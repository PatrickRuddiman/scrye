//! Address normalization. The lowercased addr-spec is what storage's
//! sender_addr column stores and the api slice's `from` filter substring-
//! matches against, so casing has to be consistent here.

use crate::Address;

/// Pick the first usable address out of a (possibly grouped) list.
pub fn normalize(addr: &mail_parser::Address<'_>) -> Option<Address> {
    iter_normalized(addr).into_iter().next()
}

/// Flatten a (possibly grouped) list to a Vec of normalized addresses.
pub fn normalize_list(addr: &mail_parser::Address<'_>) -> Vec<Address> {
    iter_normalized(addr)
}

fn iter_normalized(a: &mail_parser::Address<'_>) -> Vec<Address> {
    let mut out = Vec::new();
    if let Some(list) = a.as_list() {
        for ad in list {
            if let Some(addr) = ad.address() {
                out.push(Address {
                    addr: addr.to_lowercase(),
                    name: ad.name().map(|n| n.to_string()),
                });
            }
        }
    }
    if let Some(groups) = a.as_group() {
        for g in groups {
            for ad in &g.addresses {
                if let Some(addr) = ad.address() {
                    out.push(Address {
                        addr: addr.to_lowercase(),
                        name: ad.name().map(|n| n.to_string()),
                    });
                }
            }
        }
    }
    out
}
