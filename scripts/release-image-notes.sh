#!/bin/sh
# Print a release body with the relay image section set. Idempotent: an existing
# section between the markers is replaced, anything else in the body is kept.
# Usage: IMAGE=... TAGS=a,b DIGEST=sha256:... release-image-notes.sh BODY_FILE
set -eu
: "${IMAGE:?}" "${TAGS:?}" "${DIGEST:?}"
body_file=$1
first=$(printf '%s' "$TAGS" | cut -d, -f1)

awk '
  /^<!-- relay-image -->$/ { skip = 1 }
  !skip { print }
  /^<!-- \/relay-image -->$/ { skip = 0 }
' "$body_file" | sed -e :a -e '/^[[:space:]]*$/{$d;N;ba' -e '}'

printf '\n<!-- relay-image -->\n## Docker image\n\n'
printf 'Public UDP relay, multi-arch (`linux/amd64`, `linux/arm64`).\n\nTags:\n\n'
printf '%s\n' "$TAGS" | tr ',' '\n' | while read -r t; do printf -- '- `%s`\n' "$t"; done
printf '\nDigest (pull by digest to pin this exact build):\n\n'
printf '```\n%s@%s\n```\n\n' "$IMAGE" "$DIGEST"
printf '```sh\ndocker pull %s\n' "$first"
printf 'docker run -d -p 7400:7400/udp -v lattice-relay:/data %s\n```\n\n' "$first"
printf 'Keep the `lattice-relay` volume: the relay key in `/data/relay.key` defines its NodeId.\n'
printf '<!-- /relay-image -->\n'
