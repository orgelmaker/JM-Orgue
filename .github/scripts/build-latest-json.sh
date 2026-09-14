#!/usr/bin/env bash
# Stelt latest.json samen uit de bestanden die AL aan de release hangen en zet
# het resultaat er als LAATSTE bij. latest.json is het startsein voor de
# updater in de app: zolang het bestand ontbreekt ziet niemand een update, en
# zo kan er nooit een update aangeboden worden waarvan de installer nog niet
# klaar is.
#
# Bron van waarheid is de RELEASE zelf en niet de workflow-artefacten: bij
# "Re-run failed jobs" draait een geslaagde bouwtaak niet opnieuw en bestaan
# zijn artefacten in die poging niet, terwijl de bestanden wél aan de release
# hangen. Daardoor is dit script ook los te draaien (zie updater-json.yml).
#
# Gebruik: GH_TOKEN=... GH_REPO=orgelmaker/JM-Orgue build-latest-json.sh v0.7.40
set -euo pipefail

TAG="${1:?gebruik: build-latest-json.sh <tag>, bijvoorbeeld v0.7.40}"
REPO="${GH_REPO:-${GITHUB_REPOSITORY:?GH_REPO of GITHUB_REPOSITORY is nodig}}"
VERSION="${TAG#v}"

tmp="$(mktemp -d)"
cd "$tmp"

# 1. Welke bestanden hangen er aan de release?
gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[].name' > assets.txt
echo "Bestanden op $TAG:"; sed 's/^/  /' assets.txt

# Waarschuwing: een pre-release of concept komt niet onder
# /releases/latest/download/ te staan; de updater vindt hem dan nooit.
SOORT="$(gh release view "$TAG" --repo "$REPO" --json isDraft,isPrerelease --jq '"\(.isDraft) \(.isPrerelease)"')"
if [ "$SOORT" != "false false" ]; then
  echo "LET OP: release $TAG is een concept of pre-release (draft/prerelease = $SOORT)."
  echo "        /releases/latest/download/latest.json wijst er dan NIET naar."
fi

pick() { grep -E "$1" assets.txt | grep -v '\.sig$' | head -1 || true; }
NSIS="$(pick '_x64-setup\.exe$')"
MSI="$(pick '_x64_[A-Za-z-]+\.msi$')"
MAC="$(pick '\.app\.tar\.gz$')"

if [ -z "$NSIS" ] || [ -z "$MSI" ]; then
  echo "FOUT: de Windows-installers staan niet (allebei) op release $TAG." >&2
  echo "  nsis=[$NSIS] msi=[$MSI]" >&2
  exit 1
fi

# 2. De handtekeningen ophalen die de bouwtaken erbij hebben gehangen.
#    `|| true` + eigen controle hieronder: gh faalt met een nietszeggende fout
#    als er geen enkel .sig-bestand is, en dan is de oorzaak juist belangrijk.
mkdir -p sigs
gh release download "$TAG" --repo "$REPO" --dir sigs --pattern '*.sig' || true
for f in "$NSIS" "$MSI"; do
  [ -s "sigs/$f.sig" ] || { echo "FOUT: handtekening voor $f ontbreekt (bouwt de CI wel met createUpdaterArtifacts?)" >&2; exit 1; }
done

# 3. latest.json samenstellen. De links staan vast op de tag, zodat bestand en
#    handtekening altijd bij elkaar horen.
url() { printf 'https://github.com/%s/releases/download/%s/%s' "$REPO" "$TAG" "$1"; }
plat() {
  jq -n --arg k "$1" --arg s "$(cat "sigs/$2.sig")" --arg u "$(url "$2")" \
     '{($k): {signature: $s, url: $u}}'
}

# pub_date uit de release zelf: een herstart levert dan byte-identieke uitvoer.
PUB="$(gh release view "$TAG" --repo "$REPO" --json publishedAt --jq .publishedAt)"

{
  # De app kent zijn eigen installatievorm en zoekt eerst
  # windows-x86_64-<installer>; windows-x86_64 is de terugval. BEIDE wijzen
  # naar de NSIS-installer: die installeert per gebruiker (geen
  # beheerdersrechten) en herstart de app zelf. De MSI wordt bewust NIET als
  # updatepad aangeboden — msiexec vraagt beheerdersrechten en laat de
  # herstart aan Windows over.
  plat windows-x86_64-nsis "$NSIS"
  plat windows-x86_64      "$NSIS"
  # macOS alleen als de mac-bouwtaak zijn update-tarball heeft geleverd. De app
  # zelf biedt op macOS geen automatische update aan (niet ondertekend/
  # genotariseerd — zie BUILDING.md); deze sleutels staan er voor het geval dat
  # ooit verandert. LET OP: 'darwin-universal' bestaat NIET in de Tauri-2-
  # updater, dus het universal-bestand moet onder BEIDE architecturen staan.
  if [ -n "$MAC" ] && [ -s "sigs/$MAC.sig" ]; then
    plat darwin-aarch64 "$MAC"
    plat darwin-x86_64  "$MAC"
  fi
} | jq -s 'add' > platforms.json

if [ -z "$MAC" ] || [ ! -s "sigs/${MAC:-geen}.sig" ]; then
  echo "LET OP: geen macOS-updatebestand (.app.tar.gz + .sig) op de release; latest.json wordt Windows-only."
fi

jq -n --arg version  "$VERSION" \
      --arg notes    "https://github.com/$REPO/releases/tag/$TAG" \
      --arg pub_date "$PUB" \
      --slurpfile platforms platforms.json \
      '{version: $version, notes: $notes, pub_date: $pub_date, platforms: $platforms[0]}' \
      > latest.json
cat latest.json

# 4. Alleen uploaden als er echt iets verandert; --clobber zodat een herstart
#    niet stukloopt op "asset bestaat al".
if gh release download "$TAG" --repo "$REPO" --dir oud --pattern 'latest.json' 2>/dev/null \
   && jq -S . oud/latest.json > a.json 2>/dev/null \
   && jq -S . latest.json > b.json \
   && cmp -s a.json b.json; then
  echo "latest.json staat er al en is identiek - niets te doen."
  exit 0
fi

gh release upload "$TAG" latest.json --repo "$REPO" --clobber
echo "latest.json geplaatst voor $TAG."
