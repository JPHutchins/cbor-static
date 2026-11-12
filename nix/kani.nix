{
  lib,
  stdenv,
  fetchurl,
  fetchCrate,
  rustPlatform,
  autoPatchelfHook,
  makeWrapper,
  runCommand,
  writeText,
  cbmc,
  kissat,
  rust-bin,
}:

let
  version = "0.68.0";
  toolchainDate = "2026-08-21";
  toolchain = rust-bin.nightly.${toolchainDate}.minimal;

  bundle = stdenv.mkDerivation {
    pname = "kani-bundle";
    inherit version;
    src = fetchurl {
      url = "https://github.com/model-checking/kani/releases/download/kani-${version}/kani-${version}-x86_64-unknown-linux-gnu.tar.gz";
      hash = "sha256-MuK0hNc+3gu/ZKLPCHnEJZQiSX2K7E7mfUSN6a54Q9M=";
    };
    nativeBuildInputs = [ autoPatchelfHook ];
    buildInputs = [
      stdenv.cc.cc.lib
      toolchain
    ];
    installPhase = ''
      [[ $(< rust-toolchain-version) == nightly-${toolchainDate}-* ]]
      rm bin/{cbmc,goto-analyzer,goto-cc,goto-instrument,kissat}
      mkdir -p $out
      cp -r . $out/kani-${version}
      ln -s ${toolchain} $out/kani-${version}/toolchain
    '';
  };

  proof =
    name: body:
    writeText "${name}.rs" ''
      #[kani::proof]
      fn ${name}() {
          let x: u8 = kani::any();
          ${body}
      }
    '';
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "kani-verifier";
  inherit version;

  src = fetchCrate {
    inherit (finalAttrs) pname version;
    hash = "sha256-Sw3+vYRXIQu2vMu54t2j/z0Ig17wePWnt2sAGDpybIw=";
  };
  cargoHash = "sha256-tD22kLRaip48ledtMsNHPbsmVEV9tmBR4LrhdplmSaM=";

  nativeBuildInputs = [ makeWrapper ];

  postInstall = ''
    for bin in kani cargo-kani; do
      wrapProgram $out/bin/$bin \
        --set KANI_HOME ${bundle} \
        --prefix PATH : ${
          lib.makeBinPath [
            cbmc
            kissat
            stdenv.cc
          ]
        }
    done
  '';

  passthru = {
    inherit bundle toolchain;
    tests.proofs = runCommand "kani-proofs" { nativeBuildInputs = [ finalAttrs.finalPackage ]; } ''
      export HOME=$TMPDIR
      kani ${proof "holds" "assert!(x.wrapping_add(1) != x);"} | tee holds.log
      grep -q "VERIFICATION:- SUCCESSFUL" holds.log
      if kani ${proof "fails" "assert!(x != 7);"} > fails.log; then exit 1; fi
      grep -q "VERIFICATION:- FAILED" fails.log
      touch $out
    '';
  };

  meta = {
    description = "Bit-precise model checker for Rust";
    homepage = "https://github.com/model-checking/kani";
    changelog = "https://github.com/model-checking/kani/releases/tag/kani-${version}";
    license = with lib.licenses; [
      mit
      asl20
    ];
    sourceProvenance = with lib.sourceTypes; [
      fromSource
      binaryNativeCode
    ];
    platforms = [ "x86_64-linux" ];
    mainProgram = "cargo-kani";
  };
})
