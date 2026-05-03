use std::collections::HashMap;

use crate::config::Config;
use crate::env::Env;
use crate::plan::Plan;
use crate::resource::ResourceId;

pub mod apc_ups;
pub mod apparmor;
pub mod apt;
pub mod ca_cert;
pub mod chrony;
pub mod common;
pub mod common_devserver;
pub mod common_tools;
pub mod common_tweaks;
pub mod devserver;
pub mod docker;
pub mod et;
pub mod flatpak;
pub mod gpg;
pub mod iptables;
pub mod libvirtd;
pub mod locales;
pub mod nix;
pub mod node_exporter;
pub mod nvidia;
pub mod nvim;
pub mod pip;
pub mod smartctl_exporter;
pub mod snapd;
pub mod systemd;
pub mod tailscale;
pub mod tuxedo;
pub mod tzdata;
pub mod ubuntu_devserver;
pub mod users;
pub mod virtualbox;
pub mod yubico;

/// Mutable working context threaded through every bundle's `build`.
///
/// It owns the `Plan` being assembled and exposes typed methods (`ctx.apt()`,
/// `ctx.users()`, ...) that lazily build each bundle, returning its `Marker`
/// `ResourceId`. Each bundle is built **at most once** per context — the memo
/// cache deduplicates so multiple downstream bundles can call `ctx.apt()` and
/// all get the same id without re-registering apt's resources.
///
/// Bundle dependencies live inside each bundle's `build` body: when a bundle
/// needs another, it just calls the corresponding method on the context.
/// There is no separate "declared deps" table to keep in sync.
#[derive(Debug)]
pub struct Context<'a> {
    pub plan: &'a mut Plan,
    pub env: &'a Env,
    pub config: &'a Config,
    cache: HashMap<&'static str, ResourceId>,
}

impl<'a> Context<'a> {
    pub fn new(plan: &'a mut Plan, env: &'a Env, config: &'a Config) -> Self {
        Self {
            plan,
            env,
            config,
            cache: HashMap::new(),
        }
    }

    pub fn apc_ups(&mut self) -> ResourceId {
        self.memoized("apc_ups", apc_ups::build)
    }

    pub fn apparmor(&mut self) -> ResourceId {
        self.memoized("apparmor", apparmor::build)
    }

    pub fn apt(&mut self) -> ResourceId {
        self.memoized("apt", apt::build)
    }

    pub fn ca_cert(&mut self) -> ResourceId {
        self.memoized("ca_cert", ca_cert::build)
    }

    pub fn common(&mut self) -> ResourceId {
        self.memoized("common", common::build)
    }

    pub fn common_devserver(&mut self) -> ResourceId {
        self.memoized("common_devserver", common_devserver::build)
    }

    pub fn common_tools(&mut self) -> ResourceId {
        self.memoized("common_tools", common_tools::build)
    }

    pub fn common_tweaks(&mut self) -> ResourceId {
        self.memoized("common_tweaks", common_tweaks::build)
    }

    pub fn devserver(&mut self) -> ResourceId {
        self.memoized("devserver", devserver::build)
    }

    pub fn docker(&mut self) -> ResourceId {
        self.memoized("docker", docker::build)
    }

    pub fn et(&mut self) -> ResourceId {
        self.memoized("et", et::build)
    }

    pub fn flatpak(&mut self) -> ResourceId {
        self.memoized("flatpak", flatpak::build)
    }

    pub fn gpg(&mut self) -> ResourceId {
        self.memoized("gpg", gpg::build)
    }

    pub fn iptables(&mut self) -> ResourceId {
        self.memoized("iptables", iptables::build)
    }

    pub fn libvirtd(&mut self) -> ResourceId {
        self.memoized("libvirtd", libvirtd::build)
    }

    pub fn locales(&mut self) -> ResourceId {
        self.memoized("locales", locales::build)
    }

    pub fn nix(&mut self) -> ResourceId {
        self.memoized("nix", nix::build)
    }

    pub fn node_exporter(&mut self) -> ResourceId {
        self.memoized("node_exporter", node_exporter::build)
    }

    pub fn nvidia(&mut self) -> ResourceId {
        self.memoized("nvidia", nvidia::build)
    }

    pub fn chrony(&mut self) -> ResourceId {
        self.memoized("chrony", chrony::build)
    }

    pub fn nvim(&mut self) -> ResourceId {
        self.memoized("nvim", nvim::build)
    }

    pub fn pip(&mut self) -> ResourceId {
        self.memoized("pip", pip::build)
    }

    pub fn smartctl_exporter(&mut self) -> ResourceId {
        self.memoized("smartctl_exporter", smartctl_exporter::build)
    }

    pub fn snapd(&mut self) -> ResourceId {
        self.memoized("snapd", snapd::build)
    }

    pub fn systemd(&mut self) -> ResourceId {
        self.memoized("systemd", systemd::build)
    }

    pub fn tailscale(&mut self) -> ResourceId {
        self.memoized("tailscale", tailscale::build)
    }

    pub fn tuxedo(&mut self) -> ResourceId {
        self.memoized("tuxedo", tuxedo::build)
    }

    pub fn tzdata(&mut self) -> ResourceId {
        self.memoized("tzdata", tzdata::build)
    }

    pub fn ubuntu_devserver(&mut self) -> ResourceId {
        self.memoized("ubuntu_devserver", ubuntu_devserver::build)
    }

    pub fn users(&mut self) -> ResourceId {
        self.memoized("users", users::build)
    }

    pub fn virtualbox(&mut self) -> ResourceId {
        self.memoized("virtualbox", virtualbox::build)
    }

    pub fn yubico(&mut self) -> ResourceId {
        self.memoized("yubico", yubico::build)
    }

    fn memoized(&mut self, key: &'static str, build: fn(&mut Self) -> ResourceId) -> ResourceId {
        if let Some(id) = self.cache.get(key) {
            return *id;
        }
        let id = build(self);
        self.cache.insert(key, id);
        id
    }
}
