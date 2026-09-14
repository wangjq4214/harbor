//! Application-owned final control-flow arbitration for native widget hosts.

use harbor_widget::effects::ControlFlowEffect;
use winit::event_loop::{ActiveEventLoop, ControlFlow};

pub(crate) fn control_flow_for_effect(effect: ControlFlowEffect) -> ControlFlow {
    match effect {
        ControlFlowEffect::Wait => ControlFlow::Wait,
        ControlFlowEffect::WaitUntil(deadline) => ControlFlow::WaitUntil(deadline),
        ControlFlowEffect::Poll => ControlFlow::Poll,
    }
}

pub(crate) fn apply_control_flow(event_loop: &ActiveEventLoop, effect: ControlFlowEffect) {
    event_loop.set_control_flow(control_flow_for_effect(effect));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn control_flow_arbitration_prefers_poll_then_earliest_deadline() {
        let now = Instant::now();
        let early = now + Duration::from_secs(1);
        let late = now + Duration::from_secs(2);

        assert_eq!(
            ControlFlowEffect::Wait.arbitrate(ControlFlowEffect::Wait),
            ControlFlowEffect::Wait
        );
        assert_eq!(
            ControlFlowEffect::WaitUntil(late).arbitrate(ControlFlowEffect::WaitUntil(early)),
            ControlFlowEffect::WaitUntil(early)
        );
        assert_eq!(
            ControlFlowEffect::WaitUntil(early).arbitrate(ControlFlowEffect::Wait),
            ControlFlowEffect::WaitUntil(early)
        );
        assert_eq!(
            ControlFlowEffect::Poll.arbitrate(ControlFlowEffect::WaitUntil(late)),
            ControlFlowEffect::Poll
        );
    }

    #[test]
    fn control_flow_mapping_preserves_each_variant() {
        let deadline = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::Wait),
            ControlFlow::Wait
        );
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::WaitUntil(deadline)),
            ControlFlow::WaitUntil(deadline)
        );
        assert_eq!(
            control_flow_for_effect(ControlFlowEffect::Poll),
            ControlFlow::Poll
        );
    }
}
