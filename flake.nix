{
  description = "cbor-static development and CI environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
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
      devShell = pkgs.mkShell {
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
          export CARGO_HOME=$root/target/cargo-home
          uv sync --locked --quiet
          export PATH=$root/.venv/bin:$PATH
        '';
      };
      checks = rust.passthru.tests // kani.passthru.tests;
    in
    {
      devShells.${system}.default = devShell;

      checks.${system} = checks;
    };
}
