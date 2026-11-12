{
  description = "cbor-static development and CI environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    cbor-test-vectors = {
      url = "github:cbor-wg/cbor-test-vectors/7e84843b646676a715d4da21c719f263e4c43440";
      flake = false;
    };
    minicbor = {
      url = "github:twittner/minicbor/0432e74b1c85edf45db3c64da9f8b0d0897cdfda";
      flake = false;
    };
    postcard = {
      url = "github:jamesmunns/postcard/718aa6a6850456017c19eeff67303c633f875736";
      flake = false;
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }@inputs:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      rust = pkgs.callPackage ./nix/toolchain-proxy.nix {
        root = ./.;
        dirs = [
          "."
          "nightly"
        ];
      };
      python = pkgs.python315;
      kani = pkgs.callPackage ./nix/kani.nix { };
      fixtures = pkgs.lib.mapAttrs' (
        name: input:
        pkgs.lib.nameValuePair (pkgs.lib.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] name)) "${input}"
      ) (pkgs.lib.filterAttrs (_: input: (input._type or null) != "flake") inputs);
      devShell = pkgs.mkShell (
        fixtures
        // {
          packages = [
            rust
            python
            pkgs.uv
            pkgs.git
            pkgs.nixfmt-tree
            kani
            pkgs.cbmc
            pkgs.cargo-mutants
            pkgs.cargo-fuzz
          ];
          CBOR_STATIC_DEV_SHELL = "1";
          UV_PYTHON = pkgs.lib.getExe python;
          UV_PYTHON_DOWNLOADS = "never";
          shellHook = ''
            root=$(git rev-parse --show-toplevel)
            [[ -f $root/nightly/rust-toolchain.toml ]] || { echo "cbor-static: enter nix develop from inside the cbor-static checkout" >&2; exit 1; }
            export CARGO_HOME=$root/target/cargo-home
            uv sync --locked --quiet
            export PATH=$root/.venv/bin:$PATH
          '';
        }
      );
      checks = rust.passthru.tests // kani.passthru.tests;
    in
    {
      devShells.${system}.default = devShell;

      checks.${system} = checks;

      packages.${system} = {
        nixcache-prepare = pkgs.callPackage ./nix/nixcache.nix { };
        nixcache-roots = pkgs.linkFarm "nixcache-roots" (
          { dev-shell = devShell.inputDerivation; } // checks
        );
      };
    };
}
