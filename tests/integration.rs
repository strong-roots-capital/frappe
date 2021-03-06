use frappe::{Signal, Sink, Stream};

#[test]
fn stream_operations() {
    let sink: Sink<i32> = Sink::new();
    let stream = sink.stream();

    let s_string = stream.map(|a| a.to_string()).collect::<Vec<_>>();
    let s_odd = stream.filter(|a| a % 2 != 0).collect::<Vec<_>>();
    let s_even_half = stream
        .filter_map(|a| if *a % 2 == 0 { Some(*a / 2) } else { None })
        .collect::<Vec<_>>();
    let (pos, neg) = stream
        .map(|a| if *a > 0 { Ok(*a) } else { Err(*a) })
        .split();
    let s_pos = pos.collect::<Vec<_>>();
    let s_neg = neg.collect::<Vec<_>>();
    let s_merged = pos.merge(&neg.map(|a| -*a)).collect::<Vec<_>>();
    let s_accum = stream
        .collect::<Vec<_>>()
        .snapshot(&stream, |s, _| s)
        .collect::<Vec<_>>();
    let s_cloned = stream.fold_clone(vec![], |mut a, v| {
        a.push(v.into_owned());
        a
    });
    let s_last_pos = stream.hold_if(0, |a| *a > 0);

    sink.feed(&[5, 8, 13, -2, 42, -33]);

    assert_eq!(s_string.sample(), ["5", "8", "13", "-2", "42", "-33"]);
    assert_eq!(s_odd.sample(), [5, 13, -33]);
    assert_eq!(s_even_half.sample(), [4, -1, 21]);
    assert_eq!(s_pos.sample(), [5, 8, 13, 42]);
    assert_eq!(s_neg.sample(), [-2, -33]);
    assert_eq!(s_merged.sample(), [5, 8, 13, 2, 42, 33]);
    assert_eq!(
        s_accum.sample(),
        [
            vec![5],
            vec![5, 8],
            vec![5, 8, 13],
            vec![5, 8, 13, -2],
            vec![5, 8, 13, -2, 42],
            vec![5, 8, 13, -2, 42, -33]
        ]
    );
    assert_eq!(s_cloned.sample(), [5, 8, 13, -2, 42, -33]);
    assert_eq!(s_last_pos.sample(), 42);
}

#[test]
fn merge_with() {
    let sink1: Sink<i32> = Sink::new();
    let sink2: Sink<f32> = Sink::new();
    let stream = sink1
        .stream()
        .merge_with(&sink2.stream(), |l| Ok(*l), |r| Err(*r));
    let result = stream.collect::<Vec<_>>();

    sink1.send(1);
    sink2.send(2.0);
    sink1.send(3);
    sink1.send(4);
    sink2.send(5.0);

    assert_eq!(result.sample(), [Ok(1), Err(2.0), Ok(3), Ok(4), Err(5.0)]);
}

#[cfg(feature = "either")]
#[test]
fn merge_with_either() {
    let sink1: Sink<i32> = Sink::new();
    let sink2: Sink<f32> = Sink::new();
    let stream = sink1
        .stream()
        .merge_with_either(&sink2.stream(), |e| e.either(|l| Ok(*l), |r| Err(*r)));
    let result = stream.collect::<Vec<_>>();

    sink1.send(1);
    sink2.send(2.0);
    sink1.send(3);
    sink1.send(4);
    sink2.send(5.0);

    assert_eq!(result.sample(), [Ok(1), Err(2.0), Ok(3), Ok(4), Err(5.0)]);
}

#[test]
fn signal_switch() {
    let signal_sink = Sink::new();
    let switched = signal_sink.stream().hold(Default::default()).switch();
    let double = switched.map(|a| a * 2);

    signal_sink.send(Signal::constant(1));
    assert_eq!(switched.sample(), 1);
    assert_eq!(double.sample(), 2);

    signal_sink.send(Signal::from_fn(|| 12));
    assert_eq!(switched.sample(), 12);
    assert_eq!(double.sample(), 24);
}

#[test]
fn cloning() {
    #[derive(Default)]
    struct Storage<T> {
        vec: Vec<T>,
        clone_count: usize,
    }

    impl<T> Storage<T> {
        fn push(mut self, a: T) -> Self {
            self.vec.push(a);
            self
        }
    }

    impl<T: Clone> Clone for Storage<T> {
        fn clone(&self) -> Self {
            Storage {
                vec: self.vec.clone(),
                clone_count: self.clone_count + 1,
            }
        }
    }

    let sink = Sink::new();
    let accum = sink.stream().fold(Storage::default(), |a, v| a.push(*v));

    sink.feed(0..5);

    let result = accum.sample();
    assert_eq!(result.vec, [0, 1, 2, 3, 4]);
    assert_eq!(result.clone_count, 1);
}

#[test]
fn filter_extra() {
    let sink = Sink::new();
    let stream = sink.stream();
    let sign_res = stream.map(|a| if *a >= 0 { Ok(*a) } else { Err(*a) });
    let even_opt = stream.map(|a| if *a % 2 == 0 { Some(*a) } else { None });
    let s_even = even_opt.filter_some().collect::<Vec<_>>();
    let s_pos = sign_res.filter_first().collect::<Vec<_>>();
    let s_neg = sign_res.filter_second().collect::<Vec<_>>();

    sink.feed(vec![1, 8, -3, 42, -66]);

    assert_eq!(s_even.sample(), [8, 42, -66]);
    assert_eq!(s_pos.sample(), [1, 8, 42]);
    assert_eq!(s_neg.sample(), [-3, -66]);
}

#[test]
fn reentrant() {
    let sink = Sink::new();
    let cloned = sink.clone();
    let sig = sink
        .stream()
        .filter_map(move |n| {
            if *n < 10 {
                cloned.send(*n + 1);
                None
            } else {
                Some(*n)
            }
        })
        .hold(0);

    sink.send(1);
    assert_eq!(sig.sample(), 10);
}

#[allow(unused_variables)]
#[test]
fn deletion() {
    use std::sync::{Arc, RwLock};

    fn stream_cell(src: &Stream<i32>, i: i32) -> (Stream<i32>, Arc<RwLock<i32>>) {
        let cell = Arc::new(RwLock::new(0));
        let cell_ = cell.clone();
        let stream = src
            .map(move |n| *n + i)
            .inspect(move |n| *cell_.write().unwrap() = *n);
        (stream, cell)
    }

    let sink = Sink::new();
    let stream = sink.stream();
    let (s1, c1) = stream_cell(&stream, 1);
    let (s2, c2) = stream_cell(&stream, 2);
    let (s3, c3) = stream_cell(&stream, 3);

    sink.send(10);
    assert_eq!(*c1.read().unwrap(), 11);
    assert_eq!(*c2.read().unwrap(), 12);
    assert_eq!(*c3.read().unwrap(), 13);

    drop(s2);
    sink.send(20);
    assert_eq!(*c1.read().unwrap(), 21);
    assert_eq!(*c2.read().unwrap(), 12);
    assert_eq!(*c3.read().unwrap(), 23);
}

#[test]
fn map_n() {
    let sink = Sink::new();
    let s_out = sink
        .stream()
        .map_n(|a, sender| {
            for _ in 0..*a {
                sender.send(*a)
            }
        })
        .collect::<Vec<_>>();

    sink.feed(0..4);

    assert_eq!(s_out.sample(), [1, 2, 2, 3, 3, 3]);
}

#[test]
fn stream_collect() {
    use std::cmp::Ordering;
    use std::collections::*;

    let sink: Sink<i32> = Sink::new();
    let stream = sink.stream();
    let s_vec: Signal<Vec<_>> = stream.collect();
    let s_vecdq: Signal<VecDeque<_>> = stream.collect();
    let s_list: Signal<LinkedList<_>> = stream.collect();
    let s_set: Signal<BTreeSet<_>> = stream.collect();
    let s_string: Signal<String> = stream.map(|v| format!("{} ", v)).collect();

    sink.feed(&[1, 3, -42, 2]);

    assert_eq!(s_vec.sample(), [1, 3, -42, 2]);
    assert_eq!(s_vecdq.sample(), [1, 3, -42, 2]);
    assert_eq!(
        s_list.sample().iter().cmp([1, 3, -42, 2].iter()),
        Ordering::Equal
    );
    assert_eq!(
        s_set.sample().iter().cmp([-42, 1, 2, 3].iter()),
        Ordering::Equal
    );
    assert_eq!(s_string.sample(), "1 3 -42 2 ");

    let sink = Sink::new();
    let s_string: Signal<String> = sink.stream().collect();

    sink.feed("abZc".chars());

    assert_eq!(s_string.sample(), "abZc");
}

#[test]
fn signal_chain() {
    let sink = Sink::new();

    let sig_a = sink.stream().hold(0);
    let sig_b = sig_a.map(move |a| a + 1);
    let sig_c = sig_b.map(|a| a * 2);
    let sig_d = sig_c.map(|a| format!("({})", a));
    let sig_e = sig_d.map(|s| s + ".-");

    assert_eq!(sig_e.sample(), "(2).-");
    assert_eq!(sig_e.sample(), "(2).-");

    sink.send(42);

    assert_eq!(sig_e.sample(), "(86).-");
    assert_eq!(sig_e.sample(), "(86).-");
}

#[test]
fn signal_threading() {
    let sink = Sink::new();
    let sig = sink.stream().hold(0);
    sink.send(2);

    let threads: Vec<_> = (0..6)
        .map(|i| {
            let sig_ = sig.clone();
            std::thread::spawn(move || sig_.map(move |x: i32| x.pow(i)).sample())
        })
        .collect();

    let result: Vec<_> = threads.into_iter().map(|th| th.join().unwrap()).collect();
    assert_eq!(result, [1, 2, 4, 8, 16, 32]);
}

#[test]
fn stream_threading() {
    let sink = Sink::new();
    let sig = sink.stream().map(|x| *x + 1).fold(1, |a, x| a * *x);

    let threads: Vec<_> = (0..6)
        .map(|i| {
            let sink_ = sink.clone();
            std::thread::spawn(move || sink_.send(i))
        })
        .collect();

    for th in threads {
        th.join().unwrap();
    }

    assert_eq!(sig.sample(), 720);
}

#[test]
fn stream_send_order() {
    let sink = Sink::new();

    let stream = sink.stream();
    stream.observe(|_| false);
    let result = stream
        .fold(0, |a, _| a + 1)
        .snapshot(&stream, |a, _| a)
        .collect::<Vec<_>>();

    sink.send(());
    sink.send(());
    sink.send(());
    assert_eq!(result.sample(), [1, 2, 3]);
}

#[cfg(feature = "lazycell")]
#[test]
fn signal_cyclic() {
    let sink = Sink::new();
    let stream = sink.stream();
    let sig = Signal::cyclic(|s| s.snapshot(&stream, |a, n| a + *n).hold(0));

    assert_eq!(sig.sample(), 0);
    sink.send(3);
    assert_eq!(sig.sample(), 3);
    sink.send(10);
    assert_eq!(sig.sample(), 13);
}
