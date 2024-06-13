//! Sweeper
//!
//! this is the recource guardian:
//!
//! 1. recycle recource get through small-heap by task setting max-running-time.
//! 2. If the task is not set `max-running-time`, it will be automatically recycled when it finishes running.

use crate::prelude::*;

use std::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd, Reverse};
use std::collections::BinaryHeap;
use std::sync::Arc;

#[derive(Default, Eq, Debug, Copy, Clone)]
/// recycle unit.
pub(crate) struct RecycleUnit {
    /// deadline.
    deadline: u64,

    /// task-id.
    task_id: u64,

    ///record-id.
    record_id: i64,
}

impl RecycleUnit {
    pub(crate) fn new(deadline: u64, task_id: u64, record_id: i64) -> Self {
        RecycleUnit {
            deadline,
            task_id,
            record_id,
        }
    }
}

impl Ord for RecycleUnit {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deadline.cmp(&other.deadline)
    }
}

impl PartialOrd for RecycleUnit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for RecycleUnit {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}

#[derive(Debug)]
///RecyclingBins is resource recycler, excute timeout task-handle.
pub(crate) struct RecyclingBins {
    /// storage all task-handle in there.
    recycle_unit_heap: AsyncMutex<BinaryHeap<Reverse<RecycleUnit>>>,

    /// use it to recieve source-data build recycle-unit.
    recycle_unit_sources: AsyncReceiver<RecycleUnit>,

    /// notify timeout-event to event-handler for cancel that.
    timer_event_sender: TimerEventSender,

    /// The runtime kind.
    runtime_kind: RuntimeKind,
}

impl RecyclingBins {
    /// construct.
    pub(crate) fn new(
        recycle_unit_sources: AsyncReceiver<RecycleUnit>,
        timer_event_sender: TimerEventSender,
        runtime_kind: RuntimeKind,
    ) -> Self {
        let recycle_unit_heap: AsyncMutex<BinaryHeap<Reverse<RecycleUnit>>> =
            AsyncMutex::new(BinaryHeap::new());
        let recycle_unit_sources = recycle_unit_sources;

        RecyclingBins {
            recycle_unit_heap,
            recycle_unit_sources,
            timer_event_sender,
            runtime_kind,
        }
    }

    /// alternate run fn between recycle and  add_recycle_unit.
    pub(crate) async fn recycle(self: Arc<Self>) {
        // get now timestamp

        // peek inside RecyclingBins has anyone meet contitions.

        // pop one of recycle_unit_heap, execute it.

        loop {
            let mut recycle_unit_heap = self.recycle_unit_heap.lock().await;

            let now: u64 = timestamp();
            let mut duration: Option<Duration> = None;
            for _ in 0..200 {
                if let Some(recycle_flag) = recycle_unit_heap.peek().map(|r| r.0.deadline <= now) {
                    if !recycle_flag {
                        duration = recycle_unit_heap
                            .peek()
                            .map(|r| r.0.deadline - now)
                            .map(Duration::from_secs);
                        break;
                    }

                    if let Some(recycle_unit) = recycle_unit_heap.pop().map(|v| v.0) {
                        //handle send-error.
                        self.send_timer_event(TimerEvent::TimeoutTask(
                            recycle_unit.task_id,
                            recycle_unit.record_id,
                        ))
                        .await;
                    }

                //send msg to event_handle.
                } else {
                    break;
                }
            }

            //drop lock.
            drop(recycle_unit_heap);
            self.yield_for_while(duration).await;
        }
    }

    pub(crate) async fn send_timer_event(&self, event: TimerEvent) {
        self.timer_event_sender
            .send(event)
            .await
            .unwrap_or_else(|e| error!(" `send_timer_event` : {}", e));
    }

    // FIXME: https://github.com/BinChengZhao/delay-timer/issues/28
    // Due to the large number of tasks upstream, timeout units can pile up exceptionally high.
    // A large amount of memory may be occupied here.
    pub(crate) async fn add_recycle_unit(self: Arc<Self>) {
        'loopLayer: loop {
            // TODO: Internal (shrink -> cycle-bins) or change the data structure.

            for _ in 0..200 {
                match self.recycle_unit_sources.recv().await {
                    Ok(recycle_unit) => {
                        let mut recycle_unit_heap = self.recycle_unit_heap.lock().await;

                        recycle_unit_heap.push(Reverse(recycle_unit));
                    }

                    Err(_) => {
                        break 'loopLayer;
                    }
                }
            }

            yield_now().await;
            // out of scope , recycle_unit_heap is auto-drop;
        }
    }

    pub(crate) async fn yield_for_while(&self, duration: Option<Duration>) {
        let duration = duration.unwrap_or_else(|| Duration::from_secs(3));
        match self.runtime_kind {
            RuntimeKind::Tokio => {
                sleep_by_tokio(duration).await;
            }
        }
    }
}
