{
  fetchurl,
  python3,
  runCommand,
  records,
}:

let
  zephyrRevision = "1b3f4f14ad2002beb743dba17b455a1f0c688639";
  zephyrTest =
    test: hash:
    fetchurl {
      url = "https://raw.githubusercontent.com/zephyrproject-rtos/zephyr/${zephyrRevision}/tests/subsys/mgmt/mcumgr/${test}/src/main.c";
      inherit hash;
    };
  echo = zephyrTest "os_mgmt_echo" "sha256-F9Oi6y6YCxaBZDb8fKPyqssGA476Gw1LSIG85Paht9E=";
  info = zephyrTest "os_mgmt_info" "sha256-kUlRqUMDmPGPSaak+NOZAXT9SialJH13opXyDgm1hmE=";
  smpVersion = zephyrTest "smp_version" "sha256-h8waopsW2fbqZMt6IU5FOKZsnWrXjlp7g0qxVhRZNYE=";
  smp = python3.pkgs.buildPythonPackage rec {
    pname = "smp";
    version = "4.3.0";
    pyproject = true;
    doCheck = false;
    src = records;
    build-system = [
      python3.pkgs.hatch-vcs
      python3.pkgs.hatchling
    ];
    dependencies = [
      python3.pkgs.cbor2
      python3.pkgs.crcmod
      python3.pkgs.pydantic
    ];
    env.SETUPTOOLS_SCM_PRETEND_VERSION = version;
    pythonImportsCheck = [ "smp" ];
  };
  python = python3.withPackages (packages: [
    packages.cyclopts
    packages.pydantic
    smp
  ]);
in
runCommand "smp-vectors" { nativeBuildInputs = [ python ]; } ''
  python -I ${./smp_vectors} \
    --records ${records}/tests/binary_regressions/records \
    --echo ${echo} \
    --info ${info} \
    --smp-version ${smpVersion} \
    --smp-rev ${records.rev} \
    --out $out
''
