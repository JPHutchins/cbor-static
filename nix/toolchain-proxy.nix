{
  lib,
  runCommand,
  writeShellApplication,
  writeText,
  coreutils,
  rust-bin,
  root,
  dirs,
}:

let
  toolchainFile = dir: root + "/${dir}/rust-toolchain.toml";
  toolchains = lib.genAttrs dirs (dir: rust-bin.fromRustupToolchainFile (toolchainFile dir));

  dispatch = writeShellApplication {
    name = "rust-toolchain-proxy";
    text = ''
      tool=''${0##*/}
      [[ ''${1-} != +* ]] || { echo "$tool: $1: the toolchain comes from the nearest rust-toolchain.toml, e.g. run in nightly/" >&2; exit 1; }
      dir=$PWD
      until [[ -f $dir/rust-toolchain.toml || -z $dir ]]; do dir=''${dir%/*}; done
      file=$dir/rust-toolchain.toml
      [[ -f $file ]] || { echo "$tool: no rust-toolchain.toml in $PWD or above it" >&2; exit 1; }
      read -r hash _ < <(${coreutils}/bin/sha256sum "$file")
      case $hash in
      ${lib.concatMapAttrsStringSep "\n" (
        dir: toolchain: "${builtins.hashFile "sha256" (toolchainFile dir)}) toolchain=${toolchain} ;;"
      ) toolchains}
      *) echo "$tool: $file is not a toolchain this flake built; re-enter nix develop" >&2; exit 1 ;;
      esac
      [[ -x $toolchain/bin/$tool ]] || { echo "$tool: not in the toolchain $file selects" >&2; exit 1; }
      PATH=$toolchain/bin:$PATH exec "$toolchain/bin/$tool" "$@"
    '';
  };

  proxies =
    runCommand "rust-toolchain-proxies"
      {
        passthru.tests = {
          directories = runCommand "rust-toolchain-proxies-directories" { } ''
            export PATH=${proxies}/bin:$PATH
            ${lib.concatMapAttrsStringSep "\n" (dir: toolchain: ''
              mkdir -p repo/${dir}/deeper/still
              cp --no-preserve=mode ${writeText "rust-toolchain.toml" (builtins.readFile (toolchainFile dir))} repo/${dir}/rust-toolchain.toml
              for at in repo/${dir} repo/${dir}/deeper/still; do
                for tool in rustc cargo; do
                  [[ "$(cd "$at" && $tool -V)" == "$(${toolchain}/bin/$tool -V)" ]]
                done
              done
              for tool in ${proxies}/bin/*; do
                tool=''${tool##*/}
                [[ -x ${toolchain}/bin/$tool ]] || {
                  [[ "$(cd repo/${dir} && $tool 2>&1)" == "$tool: not in the toolchain "*" selects" ]]
                  echo "$tool" >> absent
                }
              done
            '') toolchains}
            [[ -s absent ]]
            [[ "$(cd repo && cargo +nightly -V 2>&1)" == "cargo: +nightly: the toolchain comes from the nearest rust-toolchain.toml"* ]]
            echo edited >> repo/rust-toolchain.toml
            [[ "$(cd repo && rustc -V 2>&1)" == *"is not a toolchain this flake built"* ]]
            [[ "$(cd / && rustc -V 2>&1)" == "rustc: no rust-toolchain.toml in / or above it" ]]
            touch $out
          '';
        };
        meta = {
          description = "rustup-style directory overrides over Nix-built Rust toolchains";
          mainProgram = "cargo";
        };
      }
      ''
        mkdir -p $out/bin
        for tool in ${lib.concatMapAttrsStringSep " " (_: toolchain: "${toolchain}/bin/*") toolchains}; do
          ln -sf ${lib.getExe dispatch} "$out/bin/''${tool##*/}"
        done
      '';
in
proxies
