#!/usr/bin/env bash
set -euo pipefail

input_dir="${1:?usage: build-apt-repo.sh <deb-input-dir> <repo-root> [suite] [component]}"
repo_root="${2:?usage: build-apt-repo.sh <deb-input-dir> <repo-root> [suite] [component]}"
suite="${3:-stable}"
component="${4:-main}"

command -v dpkg-scanpackages >/dev/null 2>&1 || {
    echo "dpkg-scanpackages not found; install dpkg-dev" >&2
    exit 1
}
command -v gpg >/dev/null 2>&1 || {
    echo "gpg not found; install gnupg" >&2
    exit 1
}

if [ -z "${APT_SIGNING_KEY:-}" ] && [ "${ALLOW_UNSIGNED_APT_REPO:-}" != "1" ]; then
    echo "APT_SIGNING_KEY is required to sign the apt repository" >&2
    echo "Set ALLOW_UNSIGNED_APT_REPO=1 only for local test repositories." >&2
    exit 1
fi

mkdir -p "$repo_root/pool/${component}" "$repo_root/dists/${suite}/${component}/binary-amd64" "$repo_root/dists/${suite}/${component}/binary-arm64"
cp "$input_dir"/*.deb "$repo_root/pool/${component}/"

for arch in amd64 arm64; do
    pkg_dir="$repo_root/dists/${suite}/${component}/binary-${arch}"
    (
        cd "$repo_root"
        dpkg-scanpackages --arch "$arch" "pool/${component}" /dev/null > "dists/${suite}/${component}/binary-${arch}/Packages"
        gzip -9c "dists/${suite}/${component}/binary-${arch}/Packages" > "dists/${suite}/${component}/binary-${arch}/Packages.gz"
    )
done

release="$repo_root/dists/${suite}/Release"
now="$(date -Ru)"
cat > "$release" <<RELEASE
Origin: Blackwire
Label: Blackwire
Suite: ${suite}
Codename: ${suite}
Date: ${now}
Architectures: amd64 arm64
Components: ${component}
Description: Blackwire apt repository
RELEASE

(
    cd "$repo_root/dists/${suite}"
    {
        echo "SHA256:"
        for file in "${component}"/binary-*/Packages "${component}"/binary-*/Packages.gz; do
            size="$(wc -c < "$file" | tr -d ' ')"
            hash="$(sha256sum "$file" | awk '{print $1}')"
            printf ' %s %16s %s\n' "$hash" "$size" "$file"
        done
    } >> Release
)

if [ -n "${APT_SIGNING_KEY:-}" ]; then
    gnupg_home="$(mktemp -d)"
    cleanup() { rm -rf "$gnupg_home"; }
    trap cleanup EXIT
    chmod 700 "$gnupg_home"

    export GNUPGHOME="$gnupg_home"
    printf '%s' "$APT_SIGNING_KEY" | gpg --batch --import
    signing_key="$(gpg --batch --list-secret-keys --with-colons | awk -F: '/^sec:/ { print $5; exit }')"
    if [ -z "$signing_key" ]; then
        echo "APT_SIGNING_KEY did not contain an importable secret key" >&2
        exit 1
    fi

    gpg_args=(--batch --yes --local-user "$signing_key")
    if [ -n "${APT_SIGNING_PASSPHRASE:-}" ]; then
        gpg_args+=(--pinentry-mode loopback --passphrase "$APT_SIGNING_PASSPHRASE")
    fi

    gpg --batch --yes --export "$signing_key" > "$repo_root/blackwire-archive-keyring.gpg"
    gpg "${gpg_args[@]}" --armor --detach-sign --output "${release}.gpg" "$release"
    gpg "${gpg_args[@]}" --clearsign --output "$repo_root/dists/${suite}/InRelease" "$release"
elif [ "${ALLOW_UNSIGNED_APT_REPO:-}" = "1" ]; then
    echo "WARNING: generated unsigned apt repository because ALLOW_UNSIGNED_APT_REPO=1" >&2
fi
