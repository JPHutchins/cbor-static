/^Jones, et al\. .*\[Page [0-9]+\]$/ { next }

/^RFC 8392 +CBOR Web Token +May 2018$/ { next }

match($0, /^(A(\.[0-9]+)+)\.  /, found) {
  section = found[1]
  hex = ""
  next
}

/^ +[0-9a-f]+$/ {
  hex = hex $1
  next
}

/^ *$/ { next }

match($0, /Figure ([0-9]+): .* as Hex String$/, found) {
  appendix = tolower(section)
  gsub(/\./, "_", appendix)
  variant = section
  gsub(/\./, "", variant)
  print "appendix_" appendix "\t" section "\t" found[1] "\t" length(hex) / 2 "\t" toupper(hex) "\t" variant
  hex = ""
  next
}

{ hex = "" }
