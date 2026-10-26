use harbor_widget::{fiber::FiberId, signal::Signal};

fn subscribe(signal: &Signal<u32>, fiber: FiberId) {
    signal.subscribe(fiber);
    signal.unsubscribe(fiber);
}

fn main() {}
