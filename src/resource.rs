use std::any::Any;

use async_trait::async_trait;

use crate::env::Env;
use crate::error::BackendError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Changed {
    Yes,
    No,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BatchFamily {
    AptPackage,
    AptRepo,
    SystemctlEnable,
    Sysctl,
    KernelModule,
}

impl BatchFamily {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AptPackage => "apt-package",
            Self::AptRepo => "apt-repo",
            Self::SystemctlEnable => "systemctl-enable",
            Self::Sysctl => "sysctl",
            Self::KernelModule => "kernel-module",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub enum Skip {
    #[default]
    Never,
    InContainer,
}

impl Skip {
    #[must_use]
    pub const fn evaluate(&self, env: &Env) -> bool {
        match self {
            Self::Never => false,
            Self::InContainer => env.is_container(),
        }
    }
}

pub trait AsAny: Any + 'static {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any + 'static> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[async_trait]
pub trait Resource: AsAny + std::fmt::Debug + Send + Sync + 'static {
    fn id_hint(&self) -> String;

    fn deps(&self) -> &[ResourceId] {
        &[]
    }

    fn skip_when(&self) -> &Skip {
        &Skip::Never
    }

    fn batch_family(&self) -> Option<BatchFamily> {
        None
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError>;
}
