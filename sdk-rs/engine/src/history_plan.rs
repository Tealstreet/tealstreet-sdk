//! Command Read-Plan history ownership; completion is through guarded history.
use super::*;
use crate::request_context::Plan;

/// Unique history binding, separate from the cloneable Plan send handle.
/// Dropping it grants no terminal; the existing Plan expiry remains strict.
pub struct PlanReadCompletion {
    plan: Plan,
}

impl PlanReadCompletion {
    pub fn from_plan<P>(cx: &mut Cx<'_, P>, plan: &Plan) -> Result<Self, ContextError> {
        Ok(Self {
            plan: cx.bind_read_plan_history(plan)?,
        })
    }

    pub fn command(&self) -> CommandId {
        let Owner::Command { id, .. } = self.plan.owner() else {
            unreachable!("constructor admits only command Plans")
        };
        id
    }
}
