use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/gpg/packages.yml only. The role's configs.yml imports
// per-user GPG private keys from the ansible secrets store; secrets
// handling is out of scope for this framework, so it is intentionally
// omitted here.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg = ctx.plan.add(AptPackage {
        name: "gnupg".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "gpg:ready".to_string(),
        deps: vec![pkg],
        ..Default::default()
    })
}
