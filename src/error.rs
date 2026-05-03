use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    PlanCycle {
        resource_id_hint: String,
    },
    PlanReferencesUnknownResource {
        from_resource: String,
        unknown_dep_index: usize,
    },
    Backend {
        resource: String,
        source: BackendError,
    },
    TaskPanicked {
        context: String,
    },
    ConfigLoad {
        path: PathBuf,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    EnvDetect {
        what: &'static str,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

#[derive(Debug)]
pub struct BackendError {
    pub backend: &'static str,
    pub message: String,
    pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl BackendError {
    #[must_use]
    pub fn new(backend: &'static str, message: impl Into<String>) -> Self {
        Self {
            backend,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source<E>(backend: &'static str, message: impl Into<String>, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            backend,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlanCycle { resource_id_hint } => {
                write!(f, "plan has a dependency cycle involving `{resource_id_hint}`")
            }
            Self::PlanReferencesUnknownResource {
                from_resource,
                unknown_dep_index,
            } => write!(
                f,
                "resource `{from_resource}` declares dep on resource index {unknown_dep_index}, which is not in this plan",
            ),
            Self::Backend { resource, source } => {
                write!(f, "resource `{resource}` failed: {source}")
            }
            Self::TaskPanicked { context } => {
                write!(f, "internal task panicked or was cancelled: {context}")
            }
            Self::ConfigLoad { path, source } => {
                write!(f, "loading config `{}`: {source}", path.display())
            }
            Self::EnvDetect { what, source } => {
                write!(f, "detecting host environment ({what}): {source}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Backend { source, .. } => Some(source),
            Self::ConfigLoad { source, .. } | Self::EnvDetect { source, .. } => {
                Some(source.as_ref())
            }
            Self::PlanCycle { .. }
            | Self::PlanReferencesUnknownResource { .. }
            | Self::TaskPanicked { .. } => None,
        }
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.backend, self.message)
    }
}

impl std::error::Error for BackendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|boxed| -> &(dyn std::error::Error + 'static) { boxed.as_ref() })
    }
}
