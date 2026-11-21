{
  lib,
  fetchurl,
  runCommand,
  xmlstarlet,
}:

let
  rfc =
    number: hash:
    fetchurl {
      url = "https://www.rfc-editor.org/rfc/rfc${toString number}.xml";
      inherit hash;
    };
  cddl = "//sourcecode[translate(@type, 'CDL', 'cdl') = 'cddl']";
  documents = [
    {
      file = "rfc9052.cddl";
      xml = rfc 9052 "sha256-rAGuFf+z8XGXSfcvsVca3b096jg8uzmUl8WtkRiAo3w=";
      number = 9052;
      xpath = cddl;
      blocks = 23;
    }
    {
      file = "rfc9053.cddl";
      xml = rfc 9053 "sha256-gvKDKn/mzMxo1MpxdHEb7izyPOCnMDRb19O0GVlEWGM=";
      number = 9053;
      xpath = cddl;
      blocks = 1;
    }
    {
      file = "rfc9528.cddl";
      xml = rfc 9528 "sha256-dF6OvnLlSUmsHc08JQiF+hdU6fCp1VuehjFTc/xHcg4=";
      number = 9528;
      xpath = "//section[@pn = 'section-appendix.c.2']${cddl}";
      blocks = 1;
    }
    {
      file = "rfc9711-appendix-d.cddl";
      xml = rfc 9711 "sha256-3cnQhp30Qp7X7fg3yPtdr5yJBXkJBZ4XSb+sQGAO21k=";
      number = 9711;
      xpath = "//section[@pn = 'section-appendix.d']${cddl}";
      blocks = 3;
    }
  ];
  extract = document: ''
    xmlstarlet sel -T -t -m ${lib.escapeShellArg document.xpath} \
      -v "concat('; RFC ${toString document.number} ', ancestor::section[1]/@pn)" -n -v . -n \
      ${document.xml} > $out/${document.file}
    test "$(grep -c '^; RFC ' $out/${document.file})" = ${toString document.blocks}
  '';
in
runCommand "rfc-cddl" { nativeBuildInputs = [ xmlstarlet ]; } ''
  set -euo pipefail
  mkdir $out
  ${lib.concatMapStrings extract documents}
  if grep -l '&[a-z]*;' $out/*.cddl; then exit 1; fi
  cp $out/rfc9528.cddl $out/rfc9528-erratum-8258.cddl
  patch --no-backup-if-mismatch $out/rfc9528-erratum-8258.cddl ${./rfc9528-erratum-8258.patch}
''
