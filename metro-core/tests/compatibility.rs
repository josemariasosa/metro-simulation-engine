use metro_core::{Network, Track};

#[test]
fn legacy_network_track_path_remains_compatible_with_network_lookups() {
    let mut network = Network::new();
    let a = network.add_station("A");
    let b = network.add_station("B");
    let configured = Track {
        from: a,
        to: b,
        travel_seconds: 2,
    };
    network.add_track(configured.from, configured.to, configured.travel_seconds);

    let legacy: &Track = network.track(a, b).unwrap();
    let relocated: &Track = legacy;
    assert_eq!(relocated.from, a);
    assert_eq!(relocated.to, b);
    assert_eq!(relocated.travel_seconds, 2);
}
