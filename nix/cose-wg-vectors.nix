{
  lib,
  runCommand,
  writeText,
  examples,
}:

let
  inherit (lib)
    attrNames
    concatLists
    concatMap
    concatMapStrings
    concatStringsSep
    filterAttrs
    hasSuffix
    imap0
    length
    mapAttrsToList
    removeSuffix
    toUpper
    ;

  kinds = {
    sign = "Sign";
    sign0 = "Sign1";
    enveloped = "Encrypt";
    encrypted = "Encrypt0";
    mac = "Mac";
    mac0 = "Mac0";
  };
  roles = {
    ToBeSign_hex = "ToBeSign";
    ToMac_hex = "ToMac";
    AAD_hex = "Aad";
    Context_hex = "Context";
  };
  notCbor = [
    "CEK_hex"
    "KEK_hex"
    "Secret_hex"
  ];
  layers = [
    "recipients"
    "signers"
    "countersigners"
    "countersign0"
  ];
  failures = {
    ChangeTag = "ChangeTag";
    ChangeAttr = "ChangeAttr";
    AddProtected = "AddProtected";
    RemoveProtected = "RemoveProtected";
    ChangeProtected = "ChangeProtected";
    RemoveCBORTag = "RemoveCborTag";
    ChangeCBORTag = "ChangeCborTag";
  };
  unsentParts = [
    "apu_id"
    "apv_id"
    "pub_other"
    "priv_other"
  ];

  fail = name: message: throw "cose-wg-vectors: ${name}: ${message}";

  hex =
    name: value:
    if
      lib.mod (builtins.stringLength value) 2 == 0
      && lib.all (char: lib.hasInfix char "0123456789ABCDEF") (lib.stringToCharacters (toUpper value))
    then
      toUpper value
    else
      fail name "not hex: ${value}";

  rustBytes = value: "b\"${concatMapStrings (char: "\\x${lib.toLower char}") (hexPairs value)}\"";
  hexPairs =
    value: builtins.genList (i: builtins.substring (2 * i) 2 value) (builtins.stringLength value / 2);
  utf8 = value: "${builtins.toJSON value}.as_bytes()";

  dotted = concatStringsSep ".";

  intermediates =
    name: at: node:
    concatLists (
      mapAttrsToList (
        key: value:
        if roles ? ${key} then
          [
            {
              at = dotted at;
              role = roles.${key};
              hex = hex name value;
              file = dotted (at ++ [ roles.${key} ]) + ".cbor";
            }
          ]
        else if builtins.elem key notCbor then
          [ ]
        else if builtins.elem key layers then
          concatLists (
            imap0 (
              index: layer:
              intermediates name (
                at
                ++ [
                  key
                  (toString index)
                ]
              ) layer
            ) value
          )
        else
          fail name "unknown intermediate ${dotted (at ++ [ key ])}"
      ) node
    );

  findAll =
    key: at: node:
    if builtins.isAttrs node then
      (
        if node ? ${key} then
          [
            {
              at = dotted at;
              value = node.${key};
            }
          ]
        else
          [ ]
      )
      ++ concatLists (mapAttrsToList (name: value: findAll key (at ++ [ name ]) value) node)
    else if builtins.isList node then
      concatLists (imap0 (index: value: findAll key (at ++ [ (toString index) ]) value) node)
    else
      [ ];

  kindOf =
    name: input:
    let
      present = filterAttrs (key: _: input ? ${key}) kinds;
    in
    if length (attrNames present) == 1 then
      builtins.head (attrNames present)
    else
      fail name "expected one message kind, found ${toString (attrNames present)}";

  externalOf =
    name: kind: input:
    let
      found = findAll "external" [ ] input;
      allowed =
        at:
        at == "input.${kind}"
        || builtins.match "input\\.sign\\.signers\\.[0-9]+" at != null && kind == "sign";
      values = lib.unique (map (found: hex name found.value) found);
    in
    if builtins.any (found: !(allowed found.at)) found then
      fail name "external AAD outside input.<kind> or its signers"
    else if length values > 1 then
      fail name "external AADs differ"
    else
      lib.concatStrings values;

  unsentOf =
    name: kind: input:
    concatMap (
      found:
      let
        parts = lib.getAttrs (builtins.filter (part: found.value ? ${part}) unsentParts) found.value;
      in
      if parts == { } then
        [ ]
      else
        [
          {
            at = lib.removePrefix "input.${kind}" (lib.removePrefix "input.${kind}." found.at);
            inherit parts;
          }
        ]
    ) (findAll "unsent" [ ] input);

  failuresOf =
    name: input:
    map (op: failures.${op} or (fail name "unknown failure ${op}")) (
      concatMap (found: attrNames found.value) (findAll "failures" [ ] input)
    );

  vector =
    dir: file:
    let
      name = "${dir}/${removeSuffix ".json" file}";
      json = builtins.fromJSON (builtins.readFile "${examples}/${dir}/${file}");
      kind = kindOf name json.input;
    in
    {
      inherit name;
      kind = kinds.${kind};
      output = hex name json.output.cbor;
      intermediates = intermediates name [ ] (json.intermediates or { });
      external = externalOf name kind { input = json.input; };
      unsent = unsentOf name kind { input = json.input; };
      fail = json.fail or false;
      failures = failuresOf name { input = json.input; };
    };

  directories = attrNames (filterAttrs (_: type: type == "directory") (builtins.readDir examples));
  vectors = concatMap (
    dir:
    map (vector dir) (
      attrNames (
        filterAttrs (file: type: type == "regular" && hasSuffix ".json" file) (
          builtins.readDir "${examples}/${dir}"
        )
      )
    )
  ) directories;

  twins =
    vector:
    [
      {
        file = "output.cbor";
        hex = vector.output;
      }
    ]
    ++ vector.intermediates;

  twin = path: "include_bytes!(concat!(env!(\"COSE_WG_VECTORS\"), \"/${path}\"))";

  optionalBytes = parts: part: if parts ? ${part} then "Some(${utf8 parts.${part}})" else "None";

  manifest = writeText "manifest.rs" ''
    [${
      concatMapStrings (vector: ''
        Vector {
            name: "${vector.name}",
            kind: Kind::${vector.kind},
            output: ${twin "${vector.name}/output.cbor"},
            intermediates: &[${
              concatMapStrings (step: ''
                Intermediate { at: "${step.at}", role: Role::${step.role}, bytes: ${twin "${vector.name}/${step.file}"} },
              '') vector.intermediates
            }],
            external_aad: ${rustBytes vector.external},
            unsent: &[${
              concatMapStrings (unsent: ''
                Unsent { at: "${unsent.at}", apu_id: ${optionalBytes unsent.parts "apu_id"}, apv_id: ${optionalBytes unsent.parts "apv_id"}, pub_other: ${optionalBytes unsent.parts "pub_other"}, priv_other: ${optionalBytes unsent.parts "priv_other"} },
              '') vector.unsent
            }],
            expect: Expect::${if vector.fail then "Fail" else "Pass"}(&[${
              concatMapStrings (failure: "Failure::${failure}, ") vector.failures
            }]),
        },
      '') vectors
    }]
  '';

  ident =
    text:
    lib.concatMapStrings (char: if builtins.match "[a-z0-9]" char == null then "_" else char) (
      lib.stringToCharacters (lib.toLower text)
    );
  snake = {
    ToBeSign = "to_be_sign";
    ToMac = "to_mac";
    Aad = "aad";
    Context = "context";
  };
  distinctCases =
    file: cases:
    if length (lib.unique (map (case: case.ident) cases)) == length cases then
      cases
    else
      throw "cose-wg-vectors: ${file} has two cases with one name";
  vectorCases = distinctCases "cases.rs" (
    map (vector: {
      ident = ident vector.name;
      line = "(${ident vector.name}, \"${vector.name}\"),";
    }) vectors
  );
  intermediateCases = distinctCases "intermediate_cases.rs" (
    concatMap (
      vector:
      map (
        step:
        let
          name = ident (
            concatStringsSep "_" (
              builtins.filter (part: part != "") [
                vector.name
                step.at
                snake.${step.role}
              ]
            )
          );
        in
        {
          ident = name;
          line = "(${name}, \"${vector.name}\", \"${step.at}\", ${step.role}),";
        }
      ) vector.intermediates
    ) vectors
  );
  invocation =
    file: macro: cases:
    writeText file ''
      ${macro}! {
      ${concatMapStrings (case: "    ${case.line}\n") cases}}
    '';

  script = writeText "cose-wg-vectors.sh" (
    concatMapStrings (
      vector:
      concatMapStrings (twin: ''
        install -D /dev/null "$out/${vector.name}/${twin.file}"
        printf %s ${twin.hex} | basenc --base16 -d > "$out/${vector.name}/${twin.file}"
        test "$(stat -c %s "$out/${vector.name}/${twin.file}")" = ${
          toString (builtins.stringLength twin.hex / 2)
        }
      '') (twins vector)
    ) vectors
  );
in
runCommand "cose-wg-vectors" { } ''
  set -euo pipefail
  source ${script}
  cp ${manifest} $out/manifest.rs
  cp ${invocation "cases.rs" "cases" vectorCases} $out/cases.rs
  cp ${
    invocation "intermediate_cases.rs" "intermediate_cases" intermediateCases
  } $out/intermediate_cases.rs
''
