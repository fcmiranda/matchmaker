use nucleo::{Matcher, Config};

fn main() {
    let mut matcher = Matcher::new(Config::DEFAULT);
    matcher.set_stability(u32::MAX);
    // ... wait, I don't have nucleo dependency in a random dir.
}
