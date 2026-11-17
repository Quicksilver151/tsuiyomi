{
  description = "tsuiyomi - a TUI manga library for Mihon-compatible local libraries";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
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

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt-rfc-style);
    };
}
