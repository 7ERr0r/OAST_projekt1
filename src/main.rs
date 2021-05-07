use std::collections::BTreeMap;
use std::io::Write;
use std::fs::File;
use core::cmp::Ordering;
use std::collections::BinaryHeap;
use std::collections::VecDeque;
use rayon::prelude::*;

fn main() -> std::io::Result<()> {
    let simulations = 100000;

    let outs: Vec<Measurements> = (0..simulations).into_par_iter().map(|_|{
        let mut oast = Oast::new();

        oast.run();
        return oast.measurements
    }).collect();

    println!("predicted delay: {:.4} [s]", Oast::prediction_e_t_on_off());


    let averages: Vec<f64> = outs.iter().map(|m| m.cumulative_delay / m.processed_notifications as f64).collect();


    write_averages(&averages)?;
    let histogram = make_histogram(&averages);
    write_histogram(&histogram)?;



    let (avg, delta) = avg_confidence(&averages);

    println!("average   delay: {:.4} +- {:.4} [s]", avg, delta);

    Ok(())
}

fn avg_confidence(values: &[f64]) -> (f64, f64) {
    let len = values.len() as f64;
    let sqrt_len = len.sqrt();

    // EX = expected x = average
    let sum: f64 = values.iter().sum();
    let ex = sum / len;

    // standard deviation
    let stdev: f64 = values.iter().map(|v| (v - ex).powi(2)).sum();
    let stdev = stdev / len;

    // for 95% confidence
    let z = 1.960;

    let plus_minus_delta = z * stdev / sqrt_len;

    return (ex, plus_minus_delta)
}

fn make_histogram(outs: &[f64]) -> BTreeMap<i64, i64> {
    let mut map = BTreeMap::new();

    for avg_delay in outs {
        let key = avg_delay.floor() as i64;

        map.entry(key).and_modify(|v| *v += 1).or_insert(1);
    }

    map
}



fn write_averages(outs: &[f64])  -> std::io::Result<()> {
    let mut file = File::create("results.txt")?;

    for avg_delay in outs {
        write!(file, "{:.5}\n", avg_delay)?;
    }

    Ok(()) 
}

fn write_histogram(histogram: &BTreeMap<i64, i64>)  -> std::io::Result<()> {
    let mut file = File::create("histogram.txt")?;

    for entry in histogram {
        write!(file, "{}\t{}\n", entry.0, entry.1)?;
    }

    Ok(()) 
}

// Parametry zadania
// co ile [s] tworzy sie zdarzenie
const EXPECTED_SERVER_ON__SECONDS: f64 = 40.0; // E C_on
const EXPECTED_SERVER_OFF_SECONDS: f64 = 35.0; // E C_off

const PROBABILITY_SERVER_ON_: f64 =
    EXPECTED_SERVER_ON__SECONDS / (EXPECTED_SERVER_ON__SECONDS + EXPECTED_SERVER_OFF_SECONDS);
const PROBABILITY_SERVER_OFF: f64 =
    EXPECTED_SERVER_OFF_SECONDS / (EXPECTED_SERVER_ON__SECONDS + EXPECTED_SERVER_OFF_SECONDS);

const EVENT_PROCESSING_MU: f64 = 8.0; // mu
const EVENT_SPAWN__LAMBDA: f64 = 0.5; // lambda

const EXPECTED_EVENT_PROCESSING_SECONDS: f64 = 1.0 / EVENT_PROCESSING_MU; // D = 1 / mu
const EXPECTED_EVENT_SPAWNING___SECONDS: f64 = 1.0 / EVENT_SPAWN__LAMBDA; // 1 / lambda

const TIME_QUANTIZATION: u64 = 100000000;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Event {
    // Moment gdy przyszlo zgloszenie
    Notify,

    // Moment rozpoczecia przetwarzania
    // zgloszenia przez serwer
    ProcessingStart,

    // Moment zakonczenia przetworzenia
    // zgloszenia przez serwer
    ProcessingEnd,

    // Moment wlaczenia serwera
    ServerOn,

    // Moment wylaczenia serwera
    ServerOff,
}

// Struktura: czas i zdarzenie
#[derive(Copy, Clone, PartialEq)]
struct TimeEvent {
    time: u64,
    event: Event,
}

impl Eq for TimeEvent {}

// Implementacja kolejnosci (Order) dla TimeEvent
// zeby sortowac po czasie
impl Ord for TimeEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        other.time.cmp(&self.time)
    }
}
impl PartialOrd for TimeEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TimeEvent {
    fn add_time(&self, duration: f64) -> TimeEvent {
        TimeEvent {
            time: self.time + ((duration * TIME_QUANTIZATION as f64) as u64),
            event: self.event,
        }
    }
}

// Struktura kolejki wymaganej w zadaniu
// Wazne: to nie jest kolejka serwera
// Tutaj przechowujemy posortowane zdarzenia
struct OastQueue {
    // Heap (kopiec), PriorityQueue (kolejka priorytetowa)
    // to jedno i to samo czyli posortowana kolejka
    heap: BinaryHeap<TimeEvent>,
}

impl OastQueue {
    // Konstruktor
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    // Wymagana funkcja put
    pub fn put(&mut self, time: f64, event: Event) {
        // musimy przeliczyc czas na long int = u64
        // poniewaz sortujemy po liczbach calkowitych
        self.heap.push(TimeEvent {
            time: (time * TIME_QUANTIZATION as f64) as u64,
            event: event,
        });
    }

    // Wymagana funkcja get
    pub fn get(&mut self) -> Option<(f64, Event)> {
        // Zamieniamy czas spowrotem na float = f64
        return self
            .heap
            .pop()
            .map(|v| (v.time as f64 / TIME_QUANTIZATION as f64, v.event));
    }
}

// Zwraca losowy wynik z rozkladu wykladniczego
// z parametrem lambda
fn poisson(lambda_inverse: f64) -> f64 {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    return -lambda_inverse * (1.0 - rng.gen::<f64>()).ln();
}

#[derive(Default, Debug)]
struct Measurements {
    spawned_notifications: i64,
    processed_notifications: i64,
    cumulative_delay: f64,
}

#[derive(Default)]
struct Notification {
    time_queued: f64,
}
impl Notification {
    fn new(time_queued: f64) -> Self {
        Self { time_queued }
    }
}

#[derive(Default)]
struct SimulatedServer {
    // Czy jest teraz uruchomiony?
    running: bool,

    // Zakolejkowane zgloszenia
    queue: VecDeque<Notification>,

    // Czy teraz przetwarza zgloszenie?
    processing: bool,
}

struct Oast {
    events: OastQueue,
    measurements: Measurements,
    server: SimulatedServer,
    simulation_time: f64,
    czesc2: bool,
    debug: bool,
}
impl Oast {
    fn new() -> Self {
        Self {
            events: OastQueue::new(),
            measurements: Measurements::default(),
            server: SimulatedServer::default(),
            simulation_time: 10.0 * 3600.0,
            czesc2: true,
            debug: false,
        }
    }
    fn run(&mut self) {
        // Pierwsze zgloszenie
        self.events.put(0.0001, Event::Notify);

        if self.czesc2 {
            // Dla czesci II musimy symulowac serwer ON/OFF
            // Wrzucenie pierwszego eventu powoduje symulacje serwera
            self.events.put(0.0000, Event::ServerOn);
        } else {
            // Serwer dziala zawsze w czesci I
            self.server.running = true;
        }

        // rozbieg
        let starter_duration = EXPECTED_SERVER_OFF_SECONDS * 2.0;
        {
            // chwile symulujemy
            self.run_until(starter_duration);
            // resetujemy pomiary
            self.measurements = Measurements::default();
        }


        // wlasciwa symulacja
        self.run_until(starter_duration + self.simulation_time);

        self.print_measurements();
    }


    fn run_until(&mut self, finish_time: f64) {
        loop {
            match self.events.get() {
                None => {
                    // koniec zdarzen?
                }
                Some((t, event)) => {
                    if self.debug {
                        println!(
                            "now = {:8.2} s  server.queued = {:3} {:?}",
                            t,
                            self.server.queue.len(),
                            event
                        );
                    }
                    self.process_event(t, event);
                    if t > finish_time {
                        break;
                    }
                }
            }
        }
    }

    fn print_measurements(&mut self) {
        let m = &self.measurements;

        if false {
            println!("{:?}", m);

            println!(
                "measured time between notifications: {:.3} [s]",
                self.simulation_time / m.spawned_notifications as f64
            );
            println!(
                "average   delay: {:.3} [s]",
                m.cumulative_delay / m.processed_notifications as f64
            );
        }

        
    }

    fn prediction_e_t_on_off() -> f64 {
        let la = EVENT_SPAWN__LAMBDA; // lambda
        let mu = EVENT_PROCESSING_MU; // mu
        let p_prim = la / (mu * PROBABILITY_SERVER_ON_);
        let a = p_prim + la * EXPECTED_SERVER_OFF_SECONDS * PROBABILITY_SERVER_OFF;
        let b = (1.0 - p_prim) * la;

        let expected_t_on_off = a / b;

        return expected_t_on_off;
    }

    fn try_start_processing(&mut self, now: f64) {
        if self.server.running {
            if self.server.queue.len() == 0 {
                // brak zgloszen do przetwarzania
            } else {
                if !self.server.processing {
                    // serwer zaczyna przetwarzac natychmiast
                    self.server.processing = true;
                    self.events.put(
                        now, // teraz
                        Event::ProcessingStart,
                    );
                }
            }
        }
    }

    fn process_event(&mut self, t: f64, event: Event) {
        match event {
            Event::Notify => {
                // przyszlo zgloszenie
                self.measurements.spawned_notifications += 1;

                // tworzymy kolejne
                self.events.put(
                    t + poisson(EXPECTED_EVENT_SPAWNING___SECONDS),
                    Event::Notify,
                );

                self.server.queue.push_back(Notification::new(t));
                self.try_start_processing(t);
            }
            Event::ProcessingStart => {
                // poczatek przetwarzania

                // serwer pozniej zakonczy przetwarzanie
                self.events.put(
                    t + poisson(EXPECTED_EVENT_PROCESSING_SECONDS),
                    Event::ProcessingEnd,
                );
            }
            Event::ProcessingEnd => {
                // koniec przetwarzania

                let notification = self.server.queue.pop_back().unwrap();
                self.server.processing = false;
                self.try_start_processing(t);

                {
                    self.measurements.processed_notifications += 1;
                    let delay = t - notification.time_queued;
                    self.measurements.cumulative_delay += delay;
                }
            }
            Event::ServerOn => {
                // Serwer teraz ON wiec kolejkujemy OFF
                self.server.running = true;
                let on_duration = poisson(EXPECTED_SERVER_ON__SECONDS);
                self.events.put(t + on_duration, Event::ServerOff);
                self.try_start_processing(t);
            }
            Event::ServerOff => {
                // Serwer teraz OFF wiec kolejkujemy ON
                self.server.running = false;
                let off_duration = poisson(EXPECTED_SERVER_OFF_SECONDS);
                self.events.put(t + off_duration, Event::ServerOn);

                let new_events: Vec<TimeEvent> = self
                    .events
                    .heap
                    .iter()
                    .map(|time_event| {
                        match time_event.event {
                            // przesuniecie przetwarzanie o czas niedzialania serwera
                            Event::ProcessingStart | Event::ProcessingEnd => {
                                time_event.add_time(off_duration)
                            }
                            // reszta zdarzen bez zmian
                            _ => time_event.add_time(0.0),
                        }
                    })
                    .collect();

                // kopiujemy nowe eventy do kolejki
                self.events.heap.clear();
                self.events.heap.extend(new_events);
            }
        }
    }
}
