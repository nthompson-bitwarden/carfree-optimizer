//! E14-S06 transport tests. Counter is a protocol fixture, not a simulator.
use optimizer_bridge::{Batch, Operation, Request, MAX_FRAME_BYTES, MAX_SLOTS};
use optimizer_env::wire::{BatchEnvironment, WireTransition};
use std::io::Cursor;
#[derive(Default)]
struct Counter {
    state: u64,
    value: u64,
}
impl BatchEnvironment for Counter {
    fn reset(&mut self, seed: u64) -> Result<WireTransition, String> {
        self.state += 1;
        self.value = seed;
        Ok(self.transition())
    }
    fn step(&mut self, action: usize, state: u64) -> Result<WireTransition, String> {
        if action != 0 || state != self.state {
            return Err("invalid/stale action".into());
        }
        self.state += 1;
        self.value += 1;
        Ok(self.transition())
    }
}
impl Counter {
    fn transition(&self) -> WireTransition {
        WireTransition {
            version: 1,
            state: self.state,
            observation: vec![self.value as f64],
            action_mask: vec![true],
            reward: 0.0,
            terminated: false,
            truncated: false,
        }
    }
}
fn reset(slot: usize, seed: u64) -> Request {
    Request {
        slot,
        operation: Operation::Reset { seed },
    }
}
fn step(slot: usize, state: u64, action: usize) -> Request {
    Request {
        slot,
        operation: Operation::Step { state, action },
    }
}
#[test]
fn e14_s06_batch_matches_scalar_and_preserves_request_order() {
    let mut batch = Batch::new(vec![Counter::default(), Counter::default()]).unwrap();
    let a = batch.execute(&[reset(1, 42), reset(0, 7)]).unwrap();
    assert_eq!(a.iter().map(|r| r.slot).collect::<Vec<_>>(), vec![1, 0]);
    let b = batch.execute(&[step(0, 1, 0), step(1, 1, 0)]).unwrap();
    let mut scalar = Counter::default();
    scalar.reset(7).unwrap();
    assert_eq!(b[0].result.as_ref().unwrap(), &scalar.step(0, 1).unwrap());
    scalar.reset(42).unwrap();
    let scalar_state = scalar.state;
    assert_eq!(
        b[1].result.as_ref().unwrap().observation,
        scalar.step(0, scalar_state).unwrap().observation
    );
}
#[test]
fn e14_s06_invalid_batch_has_no_partial_execution() {
    let mut batch = Batch::new(vec![Counter::default()]).unwrap();
    assert!(batch.execute(&[reset(0, 99), reset(1, 1)]).is_err());
    assert!(batch.execute(&[reset(0, 99), reset(0, 1)]).is_err());
    assert!(batch.execute(&[]).is_err());
    let output = batch.execute(&[reset(0, 42)]).unwrap();
    assert_eq!(output[0].result.as_ref().unwrap().state, 1);
    assert!(Batch::<Counter>::new(vec![]).is_err());
    assert!(Batch::new((0..=MAX_SLOTS).map(|_| Counter::default()).collect()).is_err());
}
#[test]
fn e14_s06_slot_rejections_do_not_rollback_successful_peers() {
    let mut batch = Batch::new(vec![Counter::default(), Counter::default()]).unwrap();
    batch.execute(&[reset(0, 1), reset(1, 2)]).unwrap();
    let r = batch.execute(&[step(0, 1, 99), step(1, 1, 0)]).unwrap();
    assert!(r[0].result.as_ref().unwrap_err().contains("invalid"));
    assert_eq!(r[1].result.as_ref().unwrap().state, 2);
    assert!(batch.execute(&[step(0, 1, 0)]).unwrap()[0].result.is_ok());
}
#[test]
fn e14_s06_persistent_wire_roundtrip() {
    let input = "E14\t1\t2\nreset\t1\t42\nreset\t0\t7\nE14\t1\t2\nstep\t0\t1\t0\nstep\t1\t1\t0\n";
    let mut batch = Batch::new(vec![Counter::default(), Counter::default()]).unwrap();
    let mut output = vec![];
    batch.serve(Cursor::new(input), &mut output).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "E14\t1\t2\nok\t1\t1\t1\t0\t0\t0\t42\t1\nok\t0\t1\t1\t0\t0\t0\t7\t1\nE14\t1\t2\nok\t0\t1\t2\t0\t0\t0\t8\t1\nok\t1\t1\t2\t0\t0\t0\t43\t1\n");
}
#[test]
fn e14_s06_malformed_and_oversized_frames_are_fatal_without_mutation() {
    let oversized = format!("E14\t1\t1\n{}\n", "x".repeat(MAX_FRAME_BYTES));
    for input in [
        "E14\t2\t1\nreset\t0\t1\n",
        "E14\t1\t0\n",
        "E14\t1\t65\n",
        "E14\t1\t1\nreset\t0\t-1\n",
        "E14\t1\t2\nreset\t0\t1\nBAD\n",
        "E14\t1\t1\nreset\t0\t1",
        &oversized,
    ] {
        let mut batch = Batch::new(vec![Counter::default()]).unwrap();
        let mut output = vec![];
        assert!(batch.serve(Cursor::new(input), &mut output).is_err());
        assert!(output.is_empty());
        assert_eq!(
            batch.execute(&[reset(0, 1)]).unwrap()[0]
                .result
                .as_ref()
                .unwrap()
                .state,
            1
        );
    }
}
#[test]
fn e14_s06_extreme_finite_numbers_roundtrip_within_output_bound() {
    struct Extreme;
    impl BatchEnvironment for Extreme {
        fn reset(&mut self, _: u64) -> Result<WireTransition, String> {
            Ok(WireTransition {
                version: 1,
                state: u64::MAX,
                observation: vec![f64::from_bits(1); optimizer_bridge::MAX_FEATURES],
                action_mask: vec![true; optimizer_bridge::MAX_ACTIONS],
                reward: f64::MAX,
                terminated: false,
                truncated: false,
            })
        }
        fn step(&mut self, _: usize, _: u64) -> Result<WireTransition, String> {
            self.reset(0)
        }
    }
    let mut batch = Batch::new((0..MAX_SLOTS).map(|_| Extreme).collect()).unwrap();
    let mut input = format!("E14\t1\t{MAX_SLOTS}\n");
    for slot in 0..MAX_SLOTS {
        input.push_str(&format!("reset\t{slot}\t0\n"));
    }
    let mut output = vec![];
    batch.serve(Cursor::new(input), &mut output).unwrap();
    assert!(output.len() <= optimizer_bridge::MAX_OUTPUT_BYTES);
    for row in std::str::from_utf8(&output).unwrap().lines().skip(1) {
        let fields = row.split('\t').collect::<Vec<_>>();
        assert_eq!(
            fields[4].parse::<f64>().unwrap().to_bits(),
            f64::MAX.to_bits()
        );
        let observation = fields[7].split(',').collect::<Vec<_>>();
        assert_eq!(observation.len(), optimizer_bridge::MAX_FEATURES);
        assert!(observation
            .iter()
            .all(|v| v.len() <= 31 && v.parse::<f64>().unwrap().to_bits() == 1));
        assert_eq!(fields[8].len(), optimizer_bridge::MAX_ACTIONS);
    }
}
#[test]
fn e14_s06_output_contract_violations_are_not_serialized() {
    struct Invalid;
    impl BatchEnvironment for Invalid {
        fn reset(&mut self, _: u64) -> Result<WireTransition, String> {
            Ok(WireTransition {
                version: 1,
                state: 1,
                observation: vec![f64::NAN],
                action_mask: vec![true],
                reward: 0.0,
                terminated: false,
                truncated: false,
            })
        }
        fn step(&mut self, _: usize, _: u64) -> Result<WireTransition, String> {
            self.reset(0)
        }
    }
    let mut batch = Batch::new(vec![Invalid]).unwrap();
    let mut output = vec![];
    assert!(batch
        .serve(Cursor::new("E14\t1\t1\nreset\t0\t1\n"), &mut output)
        .is_err());
    assert!(output.is_empty());
}
