#!/usr/bin/env bash
set -e

TMPDIR=`mktemp -d`
if [[ ! "$TMPDIR" || ! -d "$TMPDIR" ]]; then
    echo "Couldn't create temporary directory"
    exit 1
fi
function cleanup {
    rm -rf "$TMPDIR"
}
trap cleanup EXIT

curl http://ardownload.adobe.com/pub/adobe/reader/unix/9.x/9.5.5/enu/AdbeRdr9.5.5-1_i386linux_enu.deb > "$TMPDIR/AdbeRdr9.5.5-1_i386linux_enu.deb"
(cd "$TMPDIR" && ar x AdbeRdr9.5.5-1_i386linux_enu.deb data.tar.gz)
mkdir -p fonts/PFM
tar xzf "$TMPDIR/data.tar.gz" --directory=fonts --strip-components=6 ./opt/Adobe/Reader9/Resource/Font/{AdobePiStd.otf,CourierStd-BoldOblique.otf,CourierStd-Bold.otf,CourierStd-Oblique.otf,CourierStd.otf,MinionPro-BoldIt.otf,MinionPro-Bold.otf,MinionPro-It.otf,MinionPro-Regular.otf,MyriadPro-BoldIt.otf,MyriadPro-Bold.otf,MyriadPro-It.otf,MyriadPro-Regular.otf,ZX______.PFB,ZY______.PFB,SY______.PFB} ./opt/Adobe/Reader9/Resource/Font/PFM/{zx______.pfm,zy______.pfm,SY______.PFM}
export STANDARD_FONTS=$pwd/fonts
