use qsf_tools::ToolContext;

use crate::project_docs::ProjectDocService;
use crate::session::SessionState;

use super::{ProjectDocToolContext, ResponderToolContext, SessionToolContext};

pub trait ToolContextAccess {
    fn session_state(&self) -> Option<&SessionState>;
    fn project_doc_service(&self) -> Option<&ProjectDocService>;
}

impl<T: ToolContext + ?Sized> ToolContextAccess for T {
    fn session_state(&self) -> Option<&SessionState> {
        let any = self.as_any();
        any.downcast_ref::<SessionToolContext>()
            .map(|context| context.state.as_ref())
            .or_else(|| {
                any.downcast_ref::<ResponderToolContext>()
                    .map(|context| context.state.as_ref())
            })
    }

    fn project_doc_service(&self) -> Option<&ProjectDocService> {
        let any = self.as_any();
        any.downcast_ref::<ProjectDocToolContext>()
            .map(|context| context.service.as_ref())
            .or_else(|| {
                any.downcast_ref::<ResponderToolContext>()
                    .map(|context| context.project_docs.as_ref())
            })
    }
}
