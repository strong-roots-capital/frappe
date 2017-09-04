#[macro_use]
extern crate frappe;
use frappe::{Sink, Stream, Signal};
use frappe::types::MaybeOwned;
use std::rc::Rc;
use std::fmt::Debug;

fn vec_cons<T: Clone>(mut v: Vec<T>, x: MaybeOwned<T>) -> Vec<T> { v.push(x.into_owned()); v }

#[test]
fn stream_operations()
{
    let sink: Sink<i32> = Sink::new();
    let stream = sink.stream();

    let s_string = stream.map(|a| a.to_string()).fold(vec![], vec_cons);
    let s_odd = stream.filter(|a| a % 2 != 0).fold(vec![], vec_cons);
    let s_even_half = stream.filter_map(|a| if *a % 2 == 0 { Some(*a / 2) } else { None }).fold(vec![], vec_cons);
    let (pos, neg) = stream.map(|a| if *a > 0 { Ok(*a) } else { Err(*a) }).split();
    let s_pos = pos.fold(vec![], vec_cons);
    let s_neg = neg.fold(vec![], vec_cons);
    let s_merged = pos.merge(&neg.map(|a| -*a)).fold(vec![], vec_cons);
    let s_accum = stream.fold(vec![], vec_cons).snapshot(&stream, |s, _| s.into_owned()).fold(vec![], vec_cons);
    let s_cloned = stream.fold_clone(vec![], vec_cons);
    let s_last_pos = stream.hold_if(0, |a| *a > 0);

    sink.feed(vec![5, 8, 13, -2, 42, -33]);

    assert_eq!(s_string.sample(), ["5", "8", "13", "-2", "42", "-33"]);
    assert_eq!(s_odd.sample(), [5, 13, -33]);
    assert_eq!(s_even_half.sample(), [4, -1, 21]);
    assert_eq!(s_pos.sample(), [5, 8, 13, 42]);
    assert_eq!(s_neg.sample(), [-2, -33]);
    assert_eq!(s_merged.sample(), [5, 8, 13, 2, 42, 33]);
    assert_eq!(s_accum.sample(), [vec![5], vec![5, 8], vec![5, 8, 13], vec![5, 8, 13, -2], vec![5, 8, 13, -2, 42], vec![5, 8, 13, -2, 42, -33]]);
    assert_eq!(s_cloned.sample(), [5, 8, 13, -2, 42, -33]);
    assert_eq!(s_last_pos.sample(), 42);
}

#[cfg(feature="either")]
#[test]
fn merge_with()
{
    let sink1: Sink<i32> = Sink::new();
    let sink2: Sink<f32> = Sink::new();
    let stream: Stream<Result<_, _>> = sink1.stream().merge_with(&sink2.stream(), |e| e.either(|l| Ok(*l), |r| Err(*r)));
    let result = stream.fold(vec![], vec_cons);

    sink1.send(1);
    sink2.send(2.0);
    sink1.send(3);
    sink1.send(4);
    sink2.send(5.0);

    assert_eq!(result.sample(), [Ok(1), Err(2.0), Ok(3), Ok(4), Err(5.0)]);
}

#[test]
fn stream_channel()
{
    use std::sync::mpsc::channel;

    let sink = Sink::new();
    let input = sink.stream().as_channel();
    let (output, result) = channel();
    let s_result = Signal::from_channel(0, result);

    let thread = std::thread::spawn(move || {
        let sink2 = Sink::new();
        let stream = sink2.stream();
        let s_sum = stream.fold(0, |a, n| a + *n);
        let doubles = stream.map(|n| *n * 2);
        let rx_doubles = doubles.as_channel();

        sink2.feed(input);

        output.send(s_sum.sample()).unwrap();
        rx_doubles
    });

    assert_eq!(s_result.sample(), 0);

    sink.feed(1..100);
    drop(sink);

    let doubles = thread.join().unwrap();
    let s_doubles = Signal::fold_channel(0, doubles, |a, n| a + n);
    assert_eq!(s_result.sample(), 4950);
    assert_eq!(s_doubles.sample(), 9900);
}


#[test]
fn signal_switch()
{
    let signal_sink = Sink::new();
    let switched = signal_sink.stream().hold(Signal::constant(0)).switch();

    signal_sink.send(Signal::constant(1));
    assert_eq!(switched.sample(), 1);

    signal_sink.send(2.into());
    assert_eq!(switched.sample(), 2);
}

#[test]
fn cloning()
{
    #[derive(Debug)]
    struct Storage<T>(Vec<T>);

    impl<T> Storage<T>
    {
        fn new() -> Self { Storage(Vec::new()) }
        fn push(mut self, a: T) -> Self { self.0.push(a); self }
    }

    impl<T: Debug> Clone for Storage<T>
    {
        fn clone(&self) -> Self { panic!("storage cloned! {:?}", self.0) }
    }

    let sink = Sink::new();
    let accum = sink.stream().fold(Storage::new(), |a, v| a.push(*v));

    sink.feed(0..5);
    accum.sample_with(|res| assert_eq!(res.0, [0, 1, 2, 3, 4]));
}

#[test]
fn filter_extra()
{
    let sink = Sink::new();
    let stream = sink.stream();
    let sign_res = stream.map(|a| if *a >= 0 { Ok(*a) } else { Err(*a) });
    let even_opt = stream.map(|a| if *a % 2 == 0 { Some(*a) } else { None });
    let s_even = even_opt.filter_some().fold(vec![], vec_cons);
    let s_pos = sign_res.filter_first().fold(vec![], vec_cons);
    let s_neg = sign_res.filter_second().fold(vec![], vec_cons);

    sink.feed(vec![1, 8, -3, 42, -66]);

    assert_eq!(s_even.sample(), [8, 42, -66]);
    assert_eq!(s_pos.sample(), [1, 8, 42]);
    assert_eq!(s_neg.sample(), [-3, -66]);
}

#[test]
fn reentrant()
{
    let sink = Sink::new();
    let cloned = sink.clone();
    let sig = sink.stream()
        .filter_map(move |n| if *n < 10 { cloned.send(*n + 1); None } else { Some(*n) })
        .hold(0);

    sink.send(1);
    assert_eq!(sig.sample(), 10);
}

#[allow(unused_variables)]
#[test]
fn deletion()
{
    use std::cell::Cell;

    fn stream_cell(src: &Stream<i32>, i: i32) -> (Stream<i32>, Rc<Cell<i32>>)
    {
        let cell = Rc::new(Cell::new(0));
        let cloned = cell.clone();
        let stream = src.map(move |n| *n + i).inspect(move |n| cloned.set(*n));
        (stream, cell)
    }

    let sink = Sink::new();
    let stream = sink.stream();
    let (s1, c1) = stream_cell(&stream, 1);
    let (s2, c2) = stream_cell(&stream, 2);
    let (s3, c3) = stream_cell(&stream, 3);

    sink.send(10);
    assert_eq!(c1.get(), 11);
    assert_eq!(c2.get(), 12);
    assert_eq!(c3.get(), 13);

    drop(s2);
    sink.send(20);
    assert_eq!(c1.get(), 21);
    assert_eq!(c2.get(), 12);
    assert_eq!(c3.get(), 23);
}

#[test]
fn map_n()
{
    let sink = Sink::new();
    let s_out = sink.stream()
        .map_n(|a, sink| for _ in 0 .. *a { sink.send(*a) })
        .fold(vec![], vec_cons);

    sink.feed(0..4);

    assert_eq!(s_out.sample(), [1, 2, 2, 3, 3, 3]);
}

#[test]
fn lift()
{
    let sink1 = Sink::new();
    let sink2 = Sink::new();
    let res = signal_lift!(|a, b| a + b, sink1.stream().hold(0), sink2.stream().hold(0));

    assert_eq!(res.sample(), 0);
    sink1.send(40);
    assert_eq!(res.sample(), 40);
    sink2.send(2);
    assert_eq!(res.sample(), 42);
}
