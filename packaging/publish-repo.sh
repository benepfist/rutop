#!/bin/sh
# Builds the dnf and apt repository for GitHub Pages from a directory of .rpm/.deb files.
#
#   packaging/publish-repo.sh <packages-dir> <site-dir>
#
# Needs createrepo_c, apt-ftparchive (apt-utils), gpg and the signing key in the gpg keyring
# (GPG_KEY_ID = its fingerprint or email). Layout of <site-dir>:
#
#   rutop.asc                         public signing key
#   rutop.repo                        dnf repo file   -> /etc/yum.repos.d/
#   rutop.sources                     apt source file -> /etc/apt/sources.list.d/
#   rpm/*.rpm, rpm/repodata/          (repomd.xml.asc = signature)
#   deb/pool/main/*.deb, deb/dists/stable/{InRelease,Release,Release.gpg,main/binary-amd64/Packages*}
set -eu

pkgs=$(cd "$1" && pwd)
mkdir -p "$2"
site=$(cd "$2" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
: "${GPG_KEY_ID:?GPG_KEY_ID must be set}"

mkdir -p "$site/rpm" "$site/deb/pool/main" "$site/deb/dists/stable/main/binary-amd64"
cp "$pkgs"/*.rpm "$site/rpm/"
cp "$pkgs"/*.deb "$site/deb/pool/main/"

# dnf
createrepo_c --update "$site/rpm"
rm -f "$site/rpm/repodata/repomd.xml.asc"
gpg --batch --yes --local-user "$GPG_KEY_ID" --armor --detach-sign "$site/rpm/repodata/repomd.xml"

# apt
cd "$site/deb"
apt-ftparchive packages pool > dists/stable/main/binary-amd64/Packages
gzip -9fk dists/stable/main/binary-amd64/Packages
rm -f dists/stable/Release dists/stable/InRelease dists/stable/Release.gpg
apt-ftparchive \
  -o APT::FTPArchive::Release::Origin=rutop \
  -o APT::FTPArchive::Release::Label=rutop \
  -o APT::FTPArchive::Release::Suite=stable \
  -o APT::FTPArchive::Release::Codename=stable \
  -o APT::FTPArchive::Release::Architectures=amd64 \
  -o APT::FTPArchive::Release::Components=main \
  release dists/stable > Release.tmp
mv Release.tmp dists/stable/Release
gpg --batch --yes --local-user "$GPG_KEY_ID" --clearsign -o dists/stable/InRelease dists/stable/Release
gpg --batch --yes --local-user "$GPG_KEY_ID" --armor --detach-sign -o dists/stable/Release.gpg dists/stable/Release

gpg --armor --export "$GPG_KEY_ID" > "$site/rutop.asc"
cp "$here/rutop.repo" "$here/rutop.sources" "$site/"
touch "$site/.nojekyll"
