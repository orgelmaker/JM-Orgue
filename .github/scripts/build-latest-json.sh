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
# Fail-closed: vóórdat latest.json ontstaat wordt elke handtekening met
# `minisign -V` gecontroleerd tegen de publieke sleutel uit tauri.conf.json op
# de tag — de sleutel die in de app van deze release zit. Klopt er één niet,
# dan stopt het script en komt er GEEN latest.json online (zie stap 2b/2c).
#
# Vereist: gh, jq, base64, minisign.
# Gebruik: GH_TOKEN=... GH_REPO=orgelmaker/JM-Orgue build-latest-json.sh v0.7.40
#          --sta-pre-release  een pre-release/concept tóch doen
#          --droog            alles doen (ook de controles) behalve uploaden
set -euo pipefail

TAG=""
STA_PRE_RELEASE=0
DROOG=0
for arg in "$@"; do
  case "$arg" in
    --sta-pre-release) STA_PRE_RELEASE=1 ;;
    --droog) DROOG=1 ;;
    -*) echo "Onbekende optie: $arg" >&2; exit 2 ;;
    *) TAG="$arg" ;;
  esac
done
: "${TAG:?gebruik: build-latest-json.sh [--sta-pre-release] [--droog] <tag>, bijvoorbeeld v0.7.40}"
REPO="${GH_REPO:-${GITHUB_REPOSITORY:?GH_REPO of GITHUB_REPOSITORY is nodig}}"
VERSION="${TAG#v}"

# Zonder minisign is er geen controle mogelijk, en zonder controle geen
# latest.json (fail-closed). In de CI: apt-get install minisign.
command -v minisign >/dev/null 2>&1 || {
  echo "FOUT: minisign ontbreekt; nodig om de handtekeningen te controleren" >&2
  echo "      (ubuntu: sudo apt-get install minisign; Windows: winget install jedisct1.minisign)" >&2
  exit 1
}

tmp="$(mktemp -d)"
cd "$tmp"

# 1. Welke bestanden hangen er aan de release?
gh release view "$TAG" --repo "$REPO" --json assets --jq '.assets[].name' > assets.txt
echo "Bestanden op $TAG:"; sed 's/^/  /' assets.txt

# Een pre-release of concept is bijna altijd een vergissing: latest.json hoort
# bij de versie die IEDEREEN krijgt aangeboden. Hangt hij aan een pre-release,
# dan wijst /releases/latest/download/latest.json er niet naar (de updater
# vindt hem nooit) én krijgt iedere gebruiker die de tag-URL wel volgt een
# versie die nog niet af is. Daarom: stoppen, tenzij het expliciet de bedoeling
# is (--sta-pre-release, bijvoorbeeld om een testgroep te bedienen).
SOORT="$(gh release view "$TAG" --repo "$REPO" --json isDraft,isPrerelease --jq '"\(.isDraft) \(.isPrerelease)"')"
if [ "$SOORT" != "false false" ]; then
  if [ "$STA_PRE_RELEASE" -eq 1 ]; then
    echo "LET OP: release $TAG is een concept of pre-release (draft/prerelease = $SOORT),"
    echo "        maar --sta-pre-release is meegegeven; latest.json wordt tóch geplaatst."
    echo "        /releases/latest/download/latest.json wijst er NIET naar."
  else
    echo "FOUT: release $TAG is een concept of pre-release (draft/prerelease = $SOORT)." >&2
    echo "      latest.json is het startsein voor de updater van ALLE gebruikers en hoort" >&2
    echo "      bij een gewone, gepubliceerde release. Zet de release op 'latest release'," >&2
    echo "      of geef --sta-pre-release mee als dit echt de bedoeling is." >&2
    exit 1
  fi
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

# 2b. De publieke sleutel waarmee de app van DEZE release controleert. Die komt
#     uit tauri.conf.json op de TAG en niet uit de checkout: updater-json.yml
#     draait op master, en na een sleutelwissel zou daar de nieuwe sleutel
#     staan terwijl de app op de tag nog de oude in zich heeft.
#
#     Codering (tauri-plugin-updater, verify_signature): plugins.updater.pubkey
#     is base64 van het complete minisign-sleutelbestand (regel 1 "untrusted
#     comment: ...", regel 2 de sleutel "RW..."). Eén keer decoderen levert dus
#     precies het bestand op dat `minisign -p` verwacht.
gh api -H 'Accept: application/vnd.github.raw+json' \
   "repos/$REPO/contents/VirtualPipeOrgan/src-tauri/tauri.conf.json?ref=$TAG" > tauri.conf.json
PUBKEY_B64="$(jq -r '.plugins.updater.pubkey // empty' tauri.conf.json)"
[ -n "$PUBKEY_B64" ] || { echo "FOUT: geen plugins.updater.pubkey in tauri.conf.json op $TAG" >&2; exit 1; }
printf '%s' "$PUBKEY_B64" | base64 -d > minisign.pub \
  || { echo "FOUT: plugins.updater.pubkey op $TAG is geen geldige base64" >&2; exit 1; }
if ! sed -n 2p minisign.pub | grep -q '^RW'; then
  echo "FOUT: plugins.updater.pubkey op $TAG decodeert niet naar een minisign-sleutelbestand:" >&2
  sed 's/^/      /' minisign.pub >&2
  exit 1
fi
echo "Publieke sleutel uit tauri.conf.json op $TAG: $(head -1 minisign.pub)"

# 2c. Elke handtekening ÉCHT controleren tegen die sleutel. tauri-cli waarschuwt
#     alleen als de privésleutel niet bij de publieke hoort of ontbreekt; het
#     resultaat zou dan een latest.json zijn waarvan iedere installatie de
#     update eerst volledig downloadt en daarna afkeurt ("Bijwerken mislukt").
#     Daarom hier hetzelfde doen als de app straks: pakket downloaden en de
#     handtekening met minisign controleren. Mislukt dat voor ook maar één
#     pakket, dan stopt het script en komt er geen latest.json.
#
#     Codering: het .sig-bestand is base64 van het complete minisign-
#     handtekeningbestand (vier regels: untrusted comment, handtekening "RUQ..."
#     = algoritme "ED" (prehashed, Blake2b-512) + key-id + ed25519-handtekening,
#     trusted comment, globale handtekening). Eén keer decoderen levert het
#     .minisig-bestand op dat `minisign -x` verwacht; de key-id erin moet
#     overeenkomen met die van de publieke sleutel (minisign meldt het anders).
TE_CONTROLEREN=("$NSIS" "$MSI")
if [ -n "$MAC" ] && [ -s "sigs/$MAC.sig" ]; then
  TE_CONTROLEREN+=("$MAC")
fi
mkdir -p pak
for f in "${TE_CONTROLEREN[@]}"; do
  gh release download "$TAG" --repo "$REPO" --dir pak --pattern "$f"
  [ -s "pak/$f" ] || { echo "FOUT: $f kon niet van release $TAG gedownload worden" >&2; exit 1; }
  base64 -d "sigs/$f.sig" > "sigs/$f.minisig" \
    || { echo "FOUT: sigs/$f.sig is geen geldige base64 (is het wel een Tauri-updater-handtekening?)" >&2; exit 1; }
  echo "Controleer handtekening van $f ..."
  if ! minisign -V -p minisign.pub -m "pak/$f" -x "sigs/$f.minisig"; then
    echo "FOUT: de handtekening van $f klopt NIET bij de publieke sleutel in tauri.conf.json op $TAG." >&2
    echo "      Iedere installatie zou deze update downloaden en daarna afkeuren. latest.json" >&2
    echo "      wordt daarom NIET geplaatst. Meestal: TAURI_SIGNING_PRIVATE_KEY(_PASSWORD) in de" >&2
    echo "      repo-secrets hoort niet bij plugins.updater.pubkey (zie BUILDING.md, Ondertekening)." >&2
    exit 1
  fi
done
echo "Alle handtekeningen kloppen (${#TE_CONTROLEREN[@]} pakketten)."

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
  # windows-x86_64-<installer>; windows-x86_64 is de terugval.
  #
  # BEIDE routes moeten erin staan. De updater kiest op de manier waarop de app
  # geïnstalleerd is: een MSI-installatie mag alleen een MSI-update krijgen en
  # een NSIS-installatie alleen een NSIS-update. Ontbreekt de msi-sleutel, dan
  # krijgt wie met de MSI installeerde de NSIS-installer aangeboden — en die
  # twee ruimen elkaar niet op: de gebruiker houdt er twee naast elkaar, en dat
  # herhaalt zich bij iedere volgende update.
  #
  # Verschil tussen de twee: msiexec (MSI) installeert voor de hele machine en
  # vraagt beheerdersrechten, en laat de herstart aan Windows over; de
  # NSIS-installer installeert per gebruiker (geen beheerdersrechten) en
  # herstart de app zelf. Daarom blijft de TERUGVAL windows-x86_64 op de
  # NSIS-installer staan: wie geen installatievorm kan melden (oudere app,
  # draagbare kopie) krijgt de installer die zonder beheerdersrechten werkt.
  plat windows-x86_64-msi  "$MSI"
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

if [ "$DROOG" -eq 1 ]; then
  echo "Droogloop (--droog): latest.json is NIET geüpload; hij staat in $tmp/latest.json."
  exit 0
fi

gh release upload "$TAG" latest.json --repo "$REPO" --clobber
echo "latest.json geplaatst voor $TAG."
