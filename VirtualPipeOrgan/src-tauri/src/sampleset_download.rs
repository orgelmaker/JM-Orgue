//! Download van een sampleset-zip: met time-outs, controle op volledigheid en
//! annulering. Los van `commands.rs` gehouden zodat het zonder Tauri te testen
//! is (de tests draaien tegen een kleine HTTP-server op 127.0.0.1).

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// Maximale tijd om de verbinding op te zetten.
pub const VERBINDEN_TIMEOUT: Duration = Duration::from_secs(20);
/// Maximale tijd dat één leesactie zonder data mag duren. Bewust géén totale
/// tijdsgrens: een grote set mag uren duren zolang er data binnenkomt.
pub const LEES_TIMEOUT: Duration = Duration::from_secs(45);
/// Maximale tijd voor één schrijfactie naar de server (alleen het verzoek).
pub const SCHRIJF_TIMEOUT: Duration = Duration::from_secs(20);

/// Agent met lees-timeout per leesactie (geen totale timeout).
pub fn maak_agent(verbinden: Duration, lezen: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(verbinden)
        .timeout_read(lezen)
        .timeout_write(SCHRIJF_TIMEOUT)
        .build()
}

/// Standaardagent voor echte downloads.
pub fn standaard_agent() -> ureq::Agent {
    maak_agent(VERBINDEN_TIMEOUT, LEES_TIMEOUT)
}

/// Foutmelding bij annuleren. Het voorvoegsel `[geannuleerd]` zorgt dat de UI
/// er geen rode foutmelding van maakt (zie `App.svelte`).
pub fn melding_geannuleerd() -> String {
    "[geannuleerd] Download afgebroken op jouw verzoek".to_string()
}

fn leesfout(e: &std::io::Error, lezen: Duration) -> String {
    use std::io::ErrorKind::{TimedOut, UnexpectedEof, WouldBlock};
    match e.kind() {
        // Windows meldt TimedOut, Unix WouldBlock bij een verlopen leestimeout.
        TimedOut | WouldBlock => format!(
            "Download vastgelopen: al {} seconden geen gegevens ontvangen",
            lezen.as_secs()
        ),
        UnexpectedEof => "Download onvolledig: de verbinding is verbroken voordat alles binnen was".to_string(),
        _ => format!("Download afgebroken: {}", e),
    }
}

/// Download `url` streamend naar `pad`. `voortgang` krijgt een percentage
/// (alleen bij een stijging; alleen als de server `Content-Length` meldt).
/// `geannuleerd` wordt vóór het verzoek en na elk gelezen blok gecontroleerd.
/// Geeft het aantal geschreven bytes terug. Een onvolledige download (minder
/// of meer bytes dan `Content-Length`) is een fout.
pub fn download_naar_bestand(
    agent: &ureq::Agent,
    url: &str,
    pad: &Path,
    lezen: Duration,
    geannuleerd: &dyn Fn() -> bool,
    voortgang: &mut dyn FnMut(u32),
) -> Result<u64, String> {
    if geannuleerd() {
        return Err(melding_geannuleerd());
    }
    let resp = agent.get(url).call().map_err(|e| format!("Download mislukt: {}", e))?;
    let totaal: u64 = resp.header("Content-Length").and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(pad).map_err(|e| format!("Zip wegschrijven mislukt: {}", e))?;
    let mut buf = [0u8; 65536];
    let mut gelezen: u64 = 0;
    let mut laatst_pct = 0u32;
    loop {
        if geannuleerd() {
            return Err(melding_geannuleerd());
        }
        let n = reader.read(&mut buf).map_err(|e| leesfout(&e, lezen))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| format!("Zip wegschrijven mislukt: {}", e))?;
        gelezen += n as u64;
        if totaal > 0 {
            let pct = ((gelezen * 100) / totaal) as u32;
            if pct > laatst_pct {
                laatst_pct = pct;
                voortgang(pct);
            }
        }
    }
    if totaal > 0 && gelezen != totaal {
        return Err(format!(
            "Download onvolledig: {} van {} bytes ontvangen",
            gelezen, totaal
        ));
    }
    Ok(gelezen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    /// Start een server die één verbinding afhandelt met `gedrag` (na het
    /// lezen van de verzoekkop) en geeft de URL terug.
    fn start_server(gedrag: impl FnOnce(TcpStream) + Send + 'static) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut buf = [0u8; 1024];
                let mut req = Vec::new();
                loop {
                    let n = s.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    req.extend_from_slice(&buf[..n]);
                    if req.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                gedrag(s);
            }
        });
        format!("http://127.0.0.1:{}/set.zip", port)
    }

    fn kop(lengte: usize) -> Vec<u8> {
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", lengte).into_bytes()
    }

    fn tijdelijk_pad(naam: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("jm-orgue-dltest-{}-{}", std::process::id(), naam));
        let _ = std::fs::remove_file(&d);
        d
    }

    const KORT: Duration = Duration::from_millis(400);

    #[test]
    fn volledige_download_schrijft_het_bestand_en_meldt_100_procent() {
        let inhoud: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let kopie = inhoud.clone();
        let url = start_server(move |mut s| {
            s.write_all(&kop(kopie.len())).unwrap();
            s.write_all(&kopie).unwrap();
        });
        let pad = tijdelijk_pad("volledig.zip");
        let mut laatste = 0u32;
        let n = download_naar_bestand(
            &maak_agent(KORT, KORT), &url, &pad, KORT, &|| false, &mut |p| laatste = p,
        ).expect("download hoort te slagen");
        assert_eq!(n, inhoud.len() as u64);
        assert_eq!(std::fs::read(&pad).unwrap(), inhoud);
        assert_eq!(laatste, 100);
        let _ = std::fs::remove_file(&pad);
    }

    #[test]
    fn afgebroken_verbinding_is_een_foutmelding_over_onvolledig() {
        let url = start_server(|mut s| {
            s.write_all(&kop(100_000)).unwrap();
            s.write_all(&vec![7u8; 40_000]).unwrap();
            // Verbinding valt hier weg (s wordt gesloten).
        });
        let pad = tijdelijk_pad("afgebroken.zip");
        let r = download_naar_bestand(&maak_agent(KORT, KORT), &url, &pad, KORT, &|| false, &mut |_| {});
        let fout = r.expect_err("een afgebroken download is geen succes");
        assert!(fout.contains("onvolledig"), "onverwachte melding: {}", fout);
        let _ = std::fs::remove_file(&pad);
    }

    #[test]
    fn stilgevallen_verbinding_loopt_op_de_leestimeout() {
        let url = start_server(|mut s| {
            s.write_all(&kop(1000)).unwrap();
            s.write_all(&[1u8; 10]).unwrap();
            // Daarna niets meer, maar de verbinding blijft open.
            std::thread::sleep(Duration::from_secs(4));
        });
        let pad = tijdelijk_pad("stil.zip");
        let start = Instant::now();
        let r = download_naar_bestand(&maak_agent(KORT, KORT), &url, &pad, KORT, &|| false, &mut |_| {});
        let fout = r.expect_err("een stilgevallen download hoort te falen");
        assert!(fout.contains("geen gegevens"), "onverwachte melding: {}", fout);
        assert!(start.elapsed() < Duration::from_secs(3), "timeout duurde te lang: {:?}", start.elapsed());
        let _ = std::fs::remove_file(&pad);
    }

    #[test]
    fn annuleren_stopt_de_download_met_de_geannuleerd_melding() {
        let url = start_server(|mut s| {
            s.write_all(&kop(640_000)).unwrap();
            for _ in 0..10 {
                if s.write_all(&vec![3u8; 64_000]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
        });
        let pad = tijdelijk_pad("annuleren.zip");
        let stop = AtomicBool::new(false);
        let r = download_naar_bestand(
            &maak_agent(KORT, KORT), &url, &pad, KORT,
            &|| stop.load(Ordering::Relaxed),
            &mut |p| if p >= 20 { stop.store(true, Ordering::Relaxed) },
        );
        let fout = r.expect_err("annuleren hoort een fout op te leveren");
        assert!(fout.starts_with("[geannuleerd]"), "onverwachte melding: {}", fout);
        let _ = std::fs::remove_file(&pad);
    }

    #[test]
    fn annulering_vooraf_doet_geen_verzoek() {
        // Poort zonder server: een verzoek zou een verbindingsfout geven.
        let pad = tijdelijk_pad("vooraf.zip");
        let r = download_naar_bestand(
            &maak_agent(KORT, KORT), "http://127.0.0.1:9/x.zip", &pad, KORT, &|| true, &mut |_| {},
        );
        assert!(r.unwrap_err().starts_with("[geannuleerd]"));
        assert!(!pad.exists());
    }
}
