// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

const JUNK_EMAILS: &[&str] = &[
    "test@test.com",
    "exmaple.com",
    "example.com",
    "example.net",
    "example.org",
    "test.com",
    "localhost",
];

const JUNK_HOSTS_AND_DOMAINS: &[&str] = &[
    "exmaple.com",
    "example.com",
    "example.net",
    "example.org",
    "test.com",
    "schemas.android.com",
    "1.2.3.4",
    "yimg.com",
    "a.b.c",
    "maps.google.com",
    "hostname",
    "localhost",
    "os.environ",
];

const JUNK_IPS: &[&str] = &["1.2.3.4"];

const JUNK_EXACT_DOMAIN_NAMES: &[&str] = &[
    "test.com",
    "something.com",
    "some.com",
    "anything.com",
    "any.com",
    "trial.com",
    "sample.com",
    "other.com",
];

const JUNK_DOMAIN_SUFFIXES: &[&str] =
    &[".png", ".jpg", ".gif", ".jpeg", ".local", ".blank", ".fill"];

const JUNK_EMAIL_AND_HOST_SUFFIXES: &[&str] = &[
    ".png", ".jpg", ".gif", ".jpeg", ".local", ".blank", ".fill", ".html", ".htm", ".txt", ".conf",
    ".cfg", ".default", ".patch", ".diff", ".tar.gz", ".tar.bz2", ".tar.xz", ".gz", ".bz2", ".xz",
    ".hint", ".sftp",
];

const JUNK_URLS: &[&str] = &[
    "http://www.adobe.com/2006/mxml",
    "http://www.w3.org/1999/xsl/transform",
    "http://docs.oasis-open.org/ns/xri/xrd-1.0",
    "http://www.w3.org/2001/xmlschema-instance",
    "http://www.w3.org/2001/xmlschema}string",
    "http://www.w3.org/2001/xmlschema",
    "http://java.sun.com/xml/ns/persistence/persistence_1_0.xsd",
    "http://bing.com",
    "http://google.com",
    "http://msn.com",
    "http://maven.apache.org/maven-v4_0_0.xsd",
    "http://maven.apache.org/pom/4.0.0",
    "http://www.w3.org/markup/dtd/xhtml-rdfa-1.dtd",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns",
    "http://www.w3.org/1999/xhtml",
    "http://www.w3.org/1999/xmlschema",
    "http://www.w3.org/1999/xmlschema-instance",
    "http://www.w3.org/2000/svg",
    "http://www.w3.org/2000/10/xmlschema",
    "http://www.w3.org/2000/10/xmlschema-instance",
    "http://www.w3.org/2002/12/soap-encoding",
    "http://www.w3.org/2002/12/soap-envelope",
    "http://www.w3.org/2005/atom",
    "http://www.w3.org/2006/01/wsdl",
    "http://www.w3.org/2006/01/wsdl/http",
    "http://www.w3.org/2006/01/wsdl/soap",
    "http://www.w3.org/2006/vcard/ns",
    "http://www.w3.org/international/o-url-and-ident.html",
    "http://www.w3.org/markup",
    "http://www.w3.org/wai/gl",
    "http://xml.apache.org/axis/session",
    "http://xml.apache.org/xml-soap",
    "http://cobertura.sourceforge.net/xml/coverage-01.dtd",
    "http://findbugs.googlecode.com/svn/trunk/findbugs/etc/docbook/docbookx.dtd",
    "http://hibernate.sourceforge.net/hibernate-configuration-2.0.dtd",
    "http://hibernate.sourceforge.net/hibernate-generic.dtd",
    "http://hibernate.sourceforge.net/hibernate-mapping-2.0.dtd",
    "http://www.opensymphony.com/xwork/xwork-1.0.dtd",
    "http://]hostname",
    "http://+",
    "http://www",
    "http://www.w3.org/hypertext/www/protocols/http/htresp.html",
    "http://www.w3.org/hypertext/www/protocols/http/object_headers.html",
    "http://www.w3.org/p3p",
    "http://www.w3.org/pub/www",
    "http://www.w3.org/tr/html4/strict.dtd",
    "http://www.w3.org/tr/rec-html40/loose.dtd",
    "http://www.w3.org/tr/xhtml1/dtd/xhtml1-strict.dtd",
    "http://www.w3.org/tr/xhtml1/dtd/xhtml1-transitional.dtd",
    "http://www.w3.org/tr/xslt",
    "https:",
    "https://+",
    "http://www.example.com",
    "http://www.example.com/dir/file",
    "http://www.example.com:dir/file",
    "http://www.your.org.here",
    "http://hostname",
    "https://www.trustedcomputinggroup.org/xml/schema/tnccs_1.0.xsd",
    "http://glade.gnome.org/glade-2.0.dtd",
    "http://pagesperso-orange.fr/sebastien.godard/sysstat.dtd",
    "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd",
    "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd",
    "http://gcc.gnu.org/bugs.html",
    "http://nsis.sf.net/nsis_error",
];

const JUNK_URL_PREFIXES: &[&str] = &[
    "http://www.springframework.org/dtd/",
    "http://www.slickedit.com/dtd/",
    "http://www.oexchange.org/spec/0.8/",
    "http://www.puppycrawl.com/dtds/",
    "http://adobe.com/as3/2006/builtin",
    "http://careers.msn.com",
    "http://foo.bar.baz",
    "http://foo.bar.com",
    "http://foobar.com",
    "http://java.sun.com/xml/ns/",
    "http://java.sun.com/j2se/1.4/docs/",
    "http://java.sun.com/j2se/1.5.0/docs/",
    "http://developer.apple.com/certificationauthority/",
    "http://www.apple.com/appleca/",
    "https://www.apple.com/certificateauthority/",
    "http://schemas.microsoft.com/",
    "http://dublincore.org/schemas/",
    "http://www.w3.org/tr/",
    "http://www.apple.com/dtds",
    "http://apache.org/xml/features/",
    "http://apache.org/xml/properties/",
    "http://crl.verisign.com/",
    "http://crl.globalsign.net/",
    "http://crl.microsoft.com/",
    "http://crl.thawte.com/",
    "http://csc3-2004-crl.verisign.com",
    "http://csc3-2009-2-crl.verisign.com",
    "http://dellincca.dell.com/crl",
    "http://ts-crl.ws.symantec.com",
    "http://java.sun.com/dtd/",
    "http://java.sun.com/j2ee/dtds/",
    "http://jakarta.apache.org/commons/dtds/",
    "http://jakarta.apache.org/struts/dtds/",
    "http://www.jboss.org/j2ee/dtd/",
    "http://glassfish.org/dtds/",
    "http://docbook.org/xml/simple/",
    "http://www.oasis-open.org/docbook/xml/",
    "http://www.w3.org/xml/1998/namespace",
    "https://www.w3.org/xml/1998/namespace",
    "http://www.w3.org/2000/xmlns/",
    "https://www.w3.org/2000/xmlns/",
    "http://ts-aia.ws.symantec.com/",
    "https://ts-aia.ws.symantec.com/",
    "https://www.verisign.com/rpa",
    "http://csc3-2010-crl.verisign.com/",
    "http://csc3-2010-aia.verisign.com/",
    "https://www.verisign.com/cps",
    "http://logo.verisign.com/",
    "http://ocsp2.globalsign.com/",
    "http://crl.globalsign.com/",
    "http://secure.globalsign.com/cacert/",
    "https://www.globalsign.com/repository/",
    "http://www.microsoft.com/pki/certs/",
    "http://www.microsoft.com/pkiops/crl",
    "http://www.microsoft.com/pki/",
];

fn classify(s: &str, data_set: &[&str], suffixes: &[&str], ignored_hosts: &[&str]) -> bool {
    if s.is_empty() {
        return false;
    }

    let normalized = s.to_lowercase().trim_end_matches('/').to_string();
    if normalized.contains('@')
        && let Some((_, host_name)) = normalized.rsplit_once('@')
        && ignored_hosts.contains(&host_name)
    {
        return false;
    }

    if data_set.iter().any(|d| normalized.contains(d)) {
        return false;
    }
    if suffixes.iter().any(|suffix| normalized.ends_with(suffix)) {
        return false;
    }

    true
}

pub(crate) fn classify_ip(ip: &str) -> bool {
    classify(ip, JUNK_IPS, &[], &[])
}

pub(crate) fn classify_host(host: &str) -> bool {
    classify(
        host,
        JUNK_HOSTS_AND_DOMAINS,
        JUNK_EMAIL_AND_HOST_SUFFIXES,
        &[],
    )
}

pub(crate) fn classify_email(email: &str) -> bool {
    classify(
        email,
        JUNK_EMAILS,
        JUNK_EMAIL_AND_HOST_SUFFIXES,
        JUNK_EXACT_DOMAIN_NAMES,
    )
}

pub(crate) fn classify_url(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }

    let normalized = url.to_lowercase().trim_end_matches('/').to_string();
    if JUNK_URLS.contains(&normalized.as_str()) {
        return false;
    }
    if JUNK_URL_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        return false;
    }
    if JUNK_DOMAIN_SUFFIXES
        .iter()
        .any(|suffix| normalized.ends_with(suffix))
    {
        return false;
    }

    true
}
