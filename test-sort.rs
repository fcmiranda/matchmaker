fn main() {
    let mut words = vec!["   0 idle", "@SPIN@   1 busy", "   2 idle"];
    words.sort();
    for w in words { println!("{}", w); }
}
