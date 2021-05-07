use core::cmp::Ordering;
use itertools_num::linspace;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::collections::BinaryHeap;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::ops::Range;

fn main() -> std::io::Result<()> {
    let oast_part: &str = &std::env::args().nth(1).unwrap_or("czesc1".to_string());

    match oast_part {
        "czesc1" => {
            czesc1(0.5..6.0, 30)?;
        }
        "czesc2" => {
            czesc2(0.5..4.0, 36)?;
        }
        "czesc2dodatek" => {
            czesc2dodatek_xy(0.5..4.0, 360)?;
        }
        _ => {}
    };

    Ok(())
}
#[derive(Eq, PartialEq, Clone, Copy)]
enum OastPart {
    Part1,
    Part2,
}

struct SystemDelayStats {
    // Parametr lambda dla ktorego mierzylismy
    lambda: f64,

    // Przewidywane opoznienie w systemie
    predicted: f64,

    // Srednie opoznienie w systemie
    average: f64,

    // Przedzial ufnosci +-
    confidence: f64,
}

fn czesc1(lambdas: Range<f64>, n: usize) -> std::io::Result<()> {
    let simulations = 200;

    let stats: Vec<SystemDelayStats> = linspace::<f64>(lambdas.start, lambdas.end, n)
        .into_iter()
        .map(|lambda| {
            let (predicted, averages) = simulate(OastPart::Part1, simulations, lambda);

            let (average, confidence) = avg_confidence(&averages);

            SystemDelayStats {
                lambda,
                predicted,
                average,
                confidence,
            }
        })
        .collect();

    write_lambda_delay_stats("czesc1_lambda_delay.txt", &stats)?;

    Ok(())
}

fn czesc2(lambdas: Range<f64>, n: usize) -> std::io::Result<()> {
    let simulations = 2000;

    let stats: Vec<SystemDelayStats> = linspace::<f64>(lambdas.start, lambdas.end, n)
        .into_iter()
        .map(|lambda| {
            let (predicted, averages) = simulate(OastPart::Part2, simulations, lambda);

            let (average, confidence) = avg_confidence(&averages);

            SystemDelayStats {
                lambda,
                predicted,
                average,
                confidence,
            }
        })
        .collect();

    write_lambda_delay_stats("czesc2_lambda_delay.txt", &stats)?;

    Ok(())
}

fn simulate(part: OastPart, simulations: usize, event_spawn_lambda: f64) -> (f64, Vec<f64>) {
    let params = JobParams {
        part: part,
        expected_server_on__seconds: 40.0,
        expected_server_off_seconds: 35.0,
        event_processing_mu: 8.0,
        event_spawn__lambda: event_spawn_lambda,
    };
    let derived = DerivedParams::new(&params);

    let outs: Vec<Measurements> = (0..simulations)
        .into_par_iter()
        .map(|_| {
            let mut oast = Oast::new(params.clone(), derived.clone());

            oast.run();
            return oast.measurements;
        })
        .collect();

    let averages: Vec<f64> = outs
        .iter()
        .map(|m| m.cumulative_delay / m.processed_notifications as f64)
        .collect();

    let (avg, delta) = avg_confidence(&averages);

    let predicted = match part {
        OastPart::Part1 => Oast::prediction_e_always_on(&params),
        OastPart::Part2 => Oast::prediction_e_t_on_off(&params, &derived),
    };

    // write_averages(&averages)?;
    // let histogram = make_histogram(&averages);
    // write_histogram(&histogram)?;

    println!("predicted delay: {:.4} [s]", predicted);

    println!("average   delay: {:.4} +- {:.4} [s]", avg, delta);

    (predicted, averages)
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

    let confidence = z * stdev / sqrt_len;

    return (ex, confidence);
}

fn write_lambda_delay_stats(filename: &str, outs: &[SystemDelayStats]) -> std::io::Result<()> {
    let mut file = File::create(filename)?;

    write!(file, "lambda\tpredicted\taverage\tconfidence\n")?;
    for s in outs {
        write!(
            file,
            "{:.5}\t{:.5}\t{:.5}\t{:.5}\n",
            s.lambda, s.predicted, s.average, s.confidence
        )?;
    }

    Ok(())
}

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

// Parametry zadania
#[allow(non_snake_case)]
#[derive(Clone)]
struct JobParams {
    part: OastPart,
    // co ile [s] tworzy sie zdarzenie
    // E C_on
    // Przykladowo: 40.0
    expected_server_on__seconds: f64,

    // E C_off
    // Przykladowo: 35.0
    expected_server_off_seconds: f64,

    // mu
    // Przykladowo: 8.0
    event_processing_mu: f64,

    // lambda
    // Przykladowo: 0.5
    event_spawn__lambda: f64,
}

// Parametry pomocnicze
#[derive(Clone)]
struct DerivedParams {
    probability_server_on_: f64,
    probability_server_off: f64,

    expected_event_processing_seconds: f64,
    expected_event_spawning___seconds: f64,
}
impl DerivedParams {
    pub fn new(params: &JobParams) -> Self {
        let on_off_sum = params.expected_server_on__seconds + params.expected_server_off_seconds;
        Self {
            probability_server_on_: params.expected_server_on__seconds / on_off_sum,
            probability_server_off: params.expected_server_off_seconds / on_off_sum,

            expected_event_processing_seconds: 1.0 / params.event_processing_mu, // d = 1 / mu
            expected_event_spawning___seconds: 1.0 / params.event_spawn__lambda, // 1 / lambda
        }
    }
}

struct Oast {
    events: OastQueue,
    measurements: Measurements,
    server: SimulatedServer,
    simulation_time: f64,
    debug: bool,

    params: JobParams,
    dparams: DerivedParams,
}
impl Oast {
    fn new(job_params: JobParams, derived_params: DerivedParams) -> Self {
        Self {
            events: OastQueue::new(),
            measurements: Measurements::default(),
            server: SimulatedServer::default(),
            simulation_time: 40.0 * 3600.0,
            debug: false,
            params: job_params,
            dparams: derived_params,
        }
    }
    fn run(&mut self) {
        // Pierwsze zgloszenie
        self.events.put(0.0001, Event::Notify);

        if self.params.part == OastPart::Part2 {
            // Dla czesci II musimy symulowac serwer ON/OFF
            // Wrzucenie pierwszego eventu powoduje symulacje serwera
            self.events.put(0.0000, Event::ServerOn);
        } else {
            // Serwer dziala zawsze w czesci I
            self.server.running = true;
        }

        // rozbieg
        let starter_duration = self.params.expected_server_off_seconds * 2.0;
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

    fn prediction_e_always_on(p: &JobParams) -> f64 {
        let la = p.event_spawn__lambda; // lambda
        let mu = p.event_processing_mu; // mu
        let p_prim = la / mu;
        let a = p_prim;
        let b = (1.0 - p_prim) * la;

        let expected_t_on_off = a / b;

        return expected_t_on_off;
    }

    fn prediction_e_t_on_off(p: &JobParams, dp: &DerivedParams) -> f64 {
        let la = p.event_spawn__lambda; // lambda
        let mu = p.event_processing_mu; // mu
        let p_prim = la / (mu * dp.probability_server_on_);
        let a = p_prim + la * p.expected_server_off_seconds * dp.probability_server_off;
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
                    t + poisson(self.dparams.expected_event_spawning___seconds),
                    Event::Notify,
                );

                self.server.queue.push_back(Notification::new(t));
                self.try_start_processing(t);
            }
            Event::ProcessingStart => {
                // poczatek przetwarzania

                // serwer pozniej zakonczy przetwarzanie
                self.events.put(
                    t + poisson(self.dparams.expected_event_processing_seconds),
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
                let on_duration = poisson(self.params.expected_server_on__seconds);
                self.events.put(t + on_duration, Event::ServerOff);
                self.try_start_processing(t);
            }
            Event::ServerOff => {
                // Serwer teraz OFF wiec kolejkujemy ON
                self.server.running = false;
                let off_duration = poisson(self.params.expected_server_off_seconds);
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

// --------- Ponizej kod niewymagany ------------

fn make_histogram(outs: &[f64]) -> BTreeMap<i64, i64> {
    let mut map = BTreeMap::new();

    for avg_delay in outs {
        let key = avg_delay.floor() as i64;

        map.entry(key).and_modify(|v| *v += 1).or_insert(1);
    }

    map
}
fn write_averages(outs: &[f64]) -> std::io::Result<()> {
    let mut file = File::create("results.txt")?;

    for avg_delay in outs {
        write!(file, "{:.5}\n", avg_delay)?;
    }

    Ok(())
}

fn write_histogram(histogram: &BTreeMap<i64, i64>) -> std::io::Result<()> {
    let mut file = File::create("histogram.txt")?;

    for entry in histogram {
        write!(file, "{}\t{}\n", entry.0, entry.1)?;
    }

    Ok(())
}

struct LambdaAndAverage {
    lambda: f64,
    average: f64,
}
fn czesc2dodatek_xy(lambdas: Range<f64>, n: usize) -> std::io::Result<()> {
    let simulations = 200;

    let mut stats: Vec<LambdaAndAverage> = Vec::new();

    for lambda in linspace::<f64>(lambdas.start, lambdas.end, n) {
        let (_predicted, averages) = simulate(OastPart::Part2, simulations, lambda);

        for average in averages {
            stats.push(LambdaAndAverage { lambda, average });
        }
    }

    write_dodatek_xy_plot("czesc2dodatek_lambda_delay.txt", &stats)?;

    Ok(())
}

fn write_dodatek_xy_plot(filename: &str, outs: &[LambdaAndAverage]) -> std::io::Result<()> {
    let mut file = File::create(filename)?;

    write!(file, "lambda\taverage\n")?;
    for s in outs {
        write!(file, "{:.5}\t{:.5}\n", s.lambda, s.average,)?;
    }

    Ok(())
}
