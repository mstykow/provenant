# BusyBox-style recipe for golden testing
SUMMARY = "Tiny versions of many common UNIX utilities in a single small executable"
DESCRIPTION = "BusyBox combines tiny versions of many common UNIX utilities \
into a single small executable."
HOMEPAGE = "https://www.busybox.net"
BUGTRACKER = "https://bugs.busybox.net/"
SECTION = "base"

LICENSE = "GPL-2.0-only"
LIC_FILES_CHKSUM = "file://LICENSE;md5=de10de48642ab74318e893a61105afbb"

SRC_URI = "https://www.busybox.net/downloads/busybox-${PV}.tar.bz2;name=tarball \
           https://www.busybox.net/downloads/fixes-${PV}/busybox-fix1.patch;name=fix1 \
           file://defconfig \
           "

DEPENDS = "virtual/libc"
RDEPENDS:${PN} = "update-alternatives"

inherit autotools update-rc.d
