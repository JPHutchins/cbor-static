{
  lib,
  writeShellApplication,
  coreutils,
  findutils,
  gawk,
  gnused,
}:

writeShellApplication {
  name = "nixcache-prepare";
  runtimeInputs = [
    coreutils
    findutils
    gawk
    gnused
  ];
  text = ''
    if (( $# < 4 )); then
      echo "usage: nixcache-prepare <cache-dir> <key-prefix> <matched-key> <installable>..." >&2
      exit 1
    fi
    case $1 in
      "" | /) echo "nixcache-prepare: refusing cache dir '$1'" >&2; exit 1 ;;
    esac
    cache_dir=$1
    key_prefix=$2
    matched_key=$3
    shift 3
    upstream=https://cache.nixos.org

    plan_store=$(realpath "$(mktemp -d)")
    trap 'chmod -R u+w "$plan_store"; rm -rf "$plan_store"' EXIT
    mapfile -t roots < <(nix path-info --derivation "$@")
    nix copy --no-check-sigs --derivation --to "$plan_store" "''${roots[@]}"
    plan_log=$(nix-store --store "$plan_store" --option substituters "$upstream" --realise --dry-run "''${roots[@]}" 2>&1)
    section() {
      awk -v want="$1" '
        /will be built:$/ { in_section = "built"; next }
        /will be fetched/ { in_section = "fetched"; next }
        in_section == want && $1 ~ "^/nix/store/" { print $1 }
      ' <<< "$plan_log" | sort -u
    }
    mapfile -t plan < <(section built)
    mapfile -t fetched < <(section fetched)
    if (( ''${#plan[@]} == 0 )); then
      echo "nixcache-prepare: empty plan; $upstream must be the only substituter while planning" >&2
      exit 1
    fi

    mapfile -t payload < <(nix-store --query --outputs "''${plan[@]}" | sort -u)
    key=$key_prefix-$(printf '%s\n' "''${payload[@]}" | sha256sum | cut -c1-16)
    echo "key=$key"
    echo "nixcache-prepare: ''${#plan[@]} derivations to build, ''${#payload[@]} outputs, ''${#fetched[@]} upstream paths, key $key" >&2
    if [[ $key == "$matched_key" ]]; then
      echo "changed=false"
      exit 0
    fi

    nix build --no-link "''${plan[@]/%/^*}"
    rm -rf -- "$cache_dir"
    nix copy --to "file://$cache_dir?compression=none" "''${roots[@]/%/^*}"
    comm -12 \
      <(find "$cache_dir" -maxdepth 1 -name '*.narinfo' -printf '%f\n' | sed 's/\.narinfo$//' | sort) \
      <(printf '%s\n' "''${fetched[@]}" | sed 's|^/nix/store/||; s|-.*||' | sort) \
      | sed "s|^|$cache_dir/|; s|$|.narinfo|" \
      | xargs -r rm -f --
    comm -23 \
      <(find "$cache_dir/nar" -type f -printf 'nar/%f\n' | sort) \
      <(sed -n 's/^URL: //p' "$cache_dir"/*.narinfo | sort -u) \
      | sed "s|^|$cache_dir/|" \
      | xargs -r rm -f --
    echo "nixcache-prepare: $(find "$cache_dir" -maxdepth 1 -name '*.narinfo' | wc -l) narinfos, $(du -sh "$cache_dir" | cut -f1)" >&2
    echo "changed=true"
  '';
  meta = {
    description = "Writes a file:// binary cache of the store paths cache.nixos.org can't serve";
    license = with lib.licenses; [
      mit
      asl20
    ];
    platforms = lib.platforms.unix;
  };
}
