{
  description = "tsuiyomi - a TUI manga library for Mihon-compatible local libraries";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # nixpkgs 26.11 dropped x86_64-darwin; the 26.05 branch still supports it.
    nixpkgs-darwin.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";
  };

  outputs = { self, nixpkgs, nixpkgs-darwin }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = system:
        if system == "x86_64-darwin"
        then nixpkgs-darwin.legacyPackages.${system}
        else nixpkgs.legacyPackages.${system};
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = pkgsFor system;
          # Platform-independent patched host jar, published once per host version.
          hostJar = pkgs.fetchurl {
            url = "https://github.com/Quicksilver151/tsuiyomi/releases/download/host-v0.2.9/miwayomi-0.2.9-patched.jar";
            hash = "sha256-yXeflYRNFLJNNHPILOYxif4PgB3241WOHzrGJ5Di1Hw=";
          };
          tsuiyomi = pkgs.rustPlatform.buildRustPackage {
            pname = "tsuiyomi";
            version = "0.2.0";

            # Only the inputs rustc and Cargo need. Documentation edits do not
            # invalidate the binary cache, and local build output or the embedded
            # Linux/macOS runtime archive never enter the store.
            src = pkgs.lib.fileset.toSource {
              root = ./.;
              fileset = pkgs.lib.fileset.difference
                (pkgs.lib.fileset.unions [
                  ./Cargo.toml
                  ./Cargo.lock
                  ./README.md
                  ./crates
                ])
                (pkgs.lib.fileset.maybeMissing ./crates/tsuiyomi-core/runtime/host.zip);
            };

            cargoLock.lockFile = ./Cargo.lock;

            # The flake uses the published host jar plus nixpkgs' JDK instead of an
            # embedded JRE. The wrapper keeps that contract visible to the binary.
            nativeBuildInputs = [ pkgs.makeWrapper ];
            doCheck = false;
            postInstall = ''
              wrapProgram $out/bin/tsuiyomi \
                --set TSUIYOMI_HOST_JAR ${hostJar} \
                --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.jdk21 ]}
            '';

            meta = {
              description = "Terminal UI for a local Mihon-compatible manga library";
              homepage = "https://github.com/Quicksilver151/tsuiyomi";
              license = pkgs.lib.licenses.mit;
              mainProgram = "tsuiyomi";
              platforms = pkgs.lib.platforms.unix;
            };
          };
        in
        {
          default = tsuiyomi;
          tsuiyomi = tsuiyomi;
        });

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/tsuiyomi";
        };
      });

      checks = forAllSystems (system: {
        default = self.packages.${system}.default;
      });

      devShells = forAllSystems (system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              # unrar_sys compiles the bundled UnRAR C++ sources.
              gcc
              rustfmt
              clippy
              rust-analyzer
              cargo-watch
              just
              # `just host-runtime` builds the extension host (Gradle + jlink).
              jdk21
              curl
              patch
            ];

            RUST_BACKTRACE = "1";

            shellHook = ''
              # Machine-local env vars (gitignored). See .env.local.example.
              if [ -f .env.local ]; then
                set -a
                . ./.env.local
                set +a
              fi
              export TSUIYOMI_DEVSHELL=1
              echo "tsuiyomi devshell: $(cargo --version)"
            '';
          };
        });

      formatter = forAllSystems (system: (pkgsFor system).nixfmt-rfc-style);

      # After the public Cachix cache exists, add:
      #   nixConfig.extra-substituters = [ "https://tsuiyomi.cachix.org" ];
      #   nixConfig.extra-trusted-public-keys = [ "tsuiyomi.cachix.org-1:..." ];
    };
}
