extern crate frappe;
use frappe::{Sink, Signal};

fn main()
{
    // values are sent from a sink..
    let sink = Sink::new();
    // ..into a stream chain
    let stream = sink.stream()
        .inspect(|a| println!("--sent: {}", a));

    // `hold` creates a Signal that stores the last value sent to the stream
    let last = stream.hold(0);
    // filter/map/fold are analogous to Iterator operations
    // most callbacks receive a Cow<T> argument, so we need to deref the value
    let sum = stream.fold(0, |acc, n| acc + *n);
    let half_even = stream
        .filter(|n| n % 2 == 0)
        .map(|n| *n / 2)
        .fold(vec![], |mut vec, n| { vec.push(*n); vec });

    // can send individual values
    sink.send(6);
    sink.send(42);
    sink.send(-1);
    // or multiple at once
    sink.feed(10..15);

    // `sample` get a copy of the value stored in the signal
    println!("last: {}", last.sample());
    println!("sum: {}", sum.sample());
    // `sample_with` gets a reference to the interval value, no clone needed
    half_even.sample_with(|v| println!("half_even: {:?}", v));
}
