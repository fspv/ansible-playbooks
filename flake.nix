{
  description = "Dev shell with all tooling needed by pre-commit hooks for this repo";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in {
        devShells.default = pkgs.mkShell {
          name = "ansible-playbooks-devshell";

          packages = with pkgs; [
            # pre-commit framework + existing yamllint hook
            pre-commit
            yamllint

            # rust toolchain for cargo-fmt and cargo-clippy hooks
            rustc
            cargo
            clippy
            rustfmt

            # act-rust-workflow pre-push hook
            act

          ];

          shellHook = ''
            if [ -d .git ] && [ -f .pre-commit-config.yaml ]; then
              pre-commit install >/dev/null 2>&1 || true
            fi
          '';
        };
      });
}
