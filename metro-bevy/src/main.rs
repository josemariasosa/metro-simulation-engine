use metro_core::network::Network;

fn main() {
    let mut network = Network::new();

    let a = network.add_station("A");
    let b = network.add_station("B");
    let c = network.add_station("C");

    println!("Network has {} stations.", network.station_count());
}
