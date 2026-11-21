{
  fetchurl,
  gawk,
  runCommand,
  writeText,
  xmlstarlet,
}:

let
  rfc9529 = fetchurl {
    url = "https://www.rfc-editor.org/rfc/rfc9529.xml";
    hash = "sha256-bHPEQci9+7kZVlH1+OwBjRJC50V7gzZZPGBEfMbkb6A=";
  };
  rfc8392 = fetchurl {
    url = "https://www.rfc-editor.org/rfc/rfc8392.txt";
    hash = "sha256-kCH/fE0nTPNIc/5PT7F8T+MjJH3Q+Wv8q1SQm/zcXxM=";
  };
  rfc8392Figures = writeText "rfc8392-figures" ''
    appendix_a_1 A.1 2 80
    appendix_a_2_1 A.2.1 4 37
    appendix_a_2_2 A.2.2 6 54
    appendix_a_2_3 A.2.3 8 132
    appendix_a_3 A.3 10 175
    appendix_a_4 A.4 12 114
    appendix_a_5 A.5 14 126
    appendix_a_6 A.6 16 221
    appendix_a_7 A.7 18 42
  '';
in
runCommand "rfc-vectors"
  {
    nativeBuildInputs = [
      gawk
      xmlstarlet
    ];
  }
  ''
    set -euo pipefail
    mkdir -p $out/rfc9529 $out/rfc8392

    xmlstarlet sel -T -t -m //artwork -v 'concat("@@ ", ancestor::section[1]/@pn)' -n -v . -n ${rfc9529} \
      | gawk -f ${./rfc9529-items.awk} > rfc9529.tsv
    test "$(grep -c '^item' rfc9529.tsv)" = 109
    test "$(grep -c '^invalid' rfc9529.tsv)" = 15
    test -z "$(cut -f2 rfc9529.tsv | sort | uniq -d)"

    tr -d '\f' < ${rfc8392} | gawk -f ${./rfc8392-figures.awk} > rfc8392.tsv
    diff <(cut -f1-4 --output-delimiter=' ' rfc8392.tsv) ${rfc8392Figures}

    twin() {
      printf %s "$2" | basenc --base16 -d > "$1"
      test "$(stat -c %s "$1")" = $(( ''${#2} / 2 ))
    }

    {
      echo 'rfc9529_cases! {'
      while IFS=$'\t' read -r kind ident section item hex; do
        [ "$kind" = item ] || continue
        twin "$out/rfc9529/$ident.cbor" "$hex"
        echo "    ($ident, \"$section\", $item),"
      done < rfc9529.tsv
      echo '}'
    } > $out/rfc9529_cases.rs

    {
      echo 'rfc9529_invalid_cases! {'
      while IFS=$'\t' read -r kind ident section item hex; do
        [ "$kind" = invalid ] || continue
        twin "$out/rfc9529/$ident.cbor" "$hex"
        echo "    ($ident, \"$section\", $item),"
      done < rfc9529.tsv
      echo '}'
    } > $out/rfc9529_invalid_cases.rs

    {
      echo 'rfc8392_cases! {'
      while IFS=$'\t' read -r ident section figure size hex variant; do
        twin "$out/rfc8392/$ident.cbor" "$hex"
        echo "    ($ident, \"$section\", $variant),"
      done < rfc8392.tsv
      echo '}'
    } > $out/rfc8392_cases.rs
  ''
