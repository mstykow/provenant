# Example BitBake recipe
SUMMARY = "Example application for testing"
DESCRIPTION = "A longer description of the example application"
HOMEPAGE = "https://example.com/project"
BUGTRACKER = "https://example.com/bugs"
SECTION = "devel"

LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://LICENSE;md5=abc123"

SRC_URI = "https://example.com/releases/example-${PV}.tar.gz \
           file://fix-build.patch \
           "

DEPENDS = "zlib openssl"
RDEPENDS:${PN} = "libz libssl"

inherit autotools pkgconfig
