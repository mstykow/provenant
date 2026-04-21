// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::net::IpAddr;

use url::Url;

use super::junk_data::{classify_host, classify_ip};

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_link_local()
                || v4.is_loopback()
                || v4.is_multicast()
                || v4.is_private()
                || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unicast_link_local()
                || v6.is_unique_local()
                || v6.is_unspecified()
        }
    }
}

pub(crate) fn is_good_host(host: &str) -> bool {
    if host.is_empty() {
        return false;
    }

    let host = host.to_lowercase();

    if let Ok(ip) = host.parse::<IpAddr>() {
        return !is_private_ip(ip) && classify_ip(&host);
    }

    if !host.contains('.') {
        return false;
    }

    classify_host(&host)
}

fn url_host_domain(url: &str) -> Option<(String, String)> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_lowercase();
    let domain = parsed
        .domain()
        .map_or_else(|| host.clone(), |d| d.to_lowercase());
    Some((host, domain))
}

pub(crate) fn is_good_email_domain(email: &str) -> bool {
    let (_, server) = match email.rsplit_once('@') {
        Some(parts) => parts,
        None => return false,
    };

    if !is_good_host(server) {
        return false;
    }

    let fake = format!("http://{server}");
    let Some((_, domain)) = url_host_domain(&fake) else {
        return false;
    };

    is_good_host(&domain)
}

pub(crate) fn is_good_url_host_domain(url: &str) -> bool {
    let Some((host, domain)) = url_host_domain(url) else {
        return false;
    };
    is_good_host(&host) && is_good_host(&domain)
}
