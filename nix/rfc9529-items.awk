function snake(text) {
  text = tolower(text)
  gsub(/[^a-z0-9]+/, "_", text)
  gsub(/^_+|_+$/, "", text)
  return text
}

function camel(text,    words, count, i, word, result) {
  count = split(text, words, /[^A-Za-z0-9]+/)
  for (i = 1; i <= count; i++) {
    word = words[i]
    result = result toupper(substr(word, 1, 1)) (word == toupper(word) ? tolower(substr(word, 2)) : substr(word, 2))
  }
  return result
}

function fail(message) {
  print "rfc9529: section " section ": " message > "/dev/stderr"
  failed = 1
  exit 1
}

function emit() {
  if (caption == "") return
  if (length(hex) != 2 * size) fail(caption ": " length(hex) / 2 " bytes, the caption says " size)
  print kind "\tsection_" snake(section) "_" snake(caption) "\t" section "\t" camel(caption) "\t" toupper(hex)
  caption = ""
  hex = ""
}

/^@@ / {
  emit()
  section = $2
  sub(/^section-/, "", section)
  next
}

match($0, /^(.+) \((CBOR Data Item|CBOR Sequence)\) \(([0-9]+) bytes?\)$/, found) {
  emit()
  if (section !~ /^[23]\./) fail("a CBOR item outside the traces: " $0)
  kind = "item"
  caption = found[1]
  size = found[3]
  next
}

match($0, /^Invalid (.+) \(([0-9]+) bytes?\)$/, found) {
  emit()
  if (section !~ /^4\./) fail("an invalid item outside section 4: " $0)
  kind = "invalid"
  caption = found[1]
  size = found[2]
  next
}

caption != "" && /^[0-9A-Fa-f]{2}( [0-9A-Fa-f]{2})*$/ {
  hex = hex $0
  gsub(/ /, "", hex)
  next
}

caption != "" && /^[[:space:]]*$/ { next }

caption != "" { fail(caption ": an unexpected line: " $0) }

END {
  if (failed) exit 1
  emit()
}
