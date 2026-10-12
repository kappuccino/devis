//! Licence : le poste se connecte au service de licences (email + mot de passe d'un compte) et
//! reçoit un ticket signé (Ed25519) valable quelques jours, renouvelé à chaque lancement avec
//! internet. Sans ticket valide pour cette machine, les commandes de l'appli sont refusées
//! (voir `lib.rs`) : seul l'écran de connexion fonctionne.
//!
//! Ticket : `<données base64url>.<signature base64url>`, données
//! `{ v, sub, email, name, device, machine, iat, exp }` (voir server/src/ticket.js).

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ring::digest::{digest, SHA256};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::State;

/// Service de licences et de mises à jour.
pub const SERVICE_URL: &str = "https://licence.cahors.kappuccino.net";
/// Clé publique des tickets (la clé privée ne quitte jamais le serveur).
const PUBLIC_KEY: &str = "cf67Ygiqn9H64qYfgJR3E0Y2RTv1pkrb4G/qOnTr0Yo=";
/// Horloge reculée de plus d'un jour par rapport à la dernière vue : ticket refusé (sinon reculer
/// la date prolongerait le fonctionnement hors ligne).
const CLOCK_TOLERANCE: i64 = 86_400;

type CmdResult<T> = Result<T, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Payload {
    pub sub: String,
    pub email: String,
    #[serde(default)]
    pub name: String,
    pub device: String,
    pub machine: String,
    pub iat: i64,
    pub exp: i64,
}

/// Enregistré dans `licence.json` (dossier de données de l'appli).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Stored {
    ticket: Option<String>,
    device_id: Option<String>,
    /// Secret du poste : renouvelle le ticket sans mot de passe. Inutile sur une autre machine
    /// (le ticket obtenu porte l'identifiant de celle-ci).
    secret: Option<String>,
    /// Date la plus récente vue (secondes) : contre le recul de l'horloge.
    #[serde(default)]
    last_seen: i64,
}

pub struct Licence {
    path: PathBuf,
    public_key: Vec<u8>,
    /// Empreinte de l'identifiant de la machine (vide si illisible : connexion impossible).
    machine: String,
    stored: Mutex<Stored>,
    /// Développement seulement : DEVIS_SANS_LICENCE=1 (jamais dans la version installée).
    bypass: bool,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Empreinte SHA-256 (hex) de l'identifiant de la machine : l'identifiant brut ne sort pas du poste.
fn machine_hash(raw: &str) -> String {
    digest(&SHA256, format!("devis-cahors:{}", raw.trim()).as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Contenu du ticket s'il est signé par `public_key`, non expiré, et émis pour cette machine.
fn verify(ticket: &str, public_key: &[u8], machine: &str, at: i64) -> Option<Payload> {
    let (data, sig) = ticket.split_once('.')?;
    let sig = URL_SAFE_NO_PAD.decode(sig).ok()?;
    UnparsedPublicKey::new(&ED25519, public_key).verify(data.as_bytes(), &sig).ok()?;
    let payload: Payload = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(data).ok()?).ok()?;
    (payload.exp > at && !machine.is_empty() && payload.machine == machine).then_some(payload)
}

impl Licence {
    pub fn open(dir: &Path) -> Self {
        let path = dir.join("licence.json");
        let stored = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let machine = machine_uid::get().map(|id| machine_hash(&id)).unwrap_or_default();
        let bypass = cfg!(debug_assertions) && std::env::var_os("DEVIS_SANS_LICENCE").is_some();
        let public_key = STANDARD.decode(PUBLIC_KEY).unwrap_or_default();
        Self { path, public_key, machine, stored: Mutex::new(stored), bypass }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Stored> {
        self.stored.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn save(&self, stored: &Stored) {
        if let Ok(json) = serde_json::to_vec_pretty(stored) {
            if let Err(e) = std::fs::write(&self.path, json) {
                eprintln!("Licence : enregistrement impossible : {e}");
            }
        }
    }

    /// Ticket valide pour ce poste, s'il y en a un.
    fn payload(&self) -> Option<Payload> {
        let mut stored = self.lock();
        let t = now();
        if t + CLOCK_TOLERANCE < stored.last_seen {
            return None;
        }
        if t > stored.last_seen {
            stored.last_seen = t;
        }
        verify(stored.ticket.as_deref()?, &self.public_key, &self.machine, t)
    }

    /// Les commandes de l'appli sont-elles autorisées ?
    pub fn is_valid(&self) -> bool {
        self.bypass || self.payload().is_some()
    }

    fn status(&self, offline: bool) -> Status {
        let payload = self.payload();
        Status {
            valid: self.bypass || payload.is_some(),
            email: payload.as_ref().map(|p| p.email.clone()),
            name: payload.as_ref().map(|p| p.name.clone()),
            expires_at: payload.as_ref().map(|p| p.exp),
            offline,
            bypass: self.bypass,
        }
    }

    fn store(&self, f: impl FnOnce(&mut Stored)) {
        let mut stored = self.lock();
        f(&mut stored);
        let copy = stored.clone();
        drop(stored);
        self.save(&copy);
    }
}

#[derive(Debug, Serialize)]
pub struct Status {
    valid: bool,
    email: Option<String>,
    name: Option<String>,
    /// Fin de validité du ticket (secondes depuis 1970) : l'appli fonctionne hors ligne jusque-là.
    expires_at: Option<i64>,
    /// Service injoignable lors du dernier renouvellement.
    offline: bool,
    /// Développement sans licence.
    bypass: bool,
}

fn client() -> CmdResult<reqwest::Client> {
    // Même fournisseur de chiffrement que le module de mise à jour (sans effet s'il est déjà installé).
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("Devis-Cahors/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

/// Réponse du service : (statut HTTP, contenu JSON), ou None si le service est injoignable.
async fn post(path: &str, body: serde_json::Value) -> CmdResult<Option<(u16, serde_json::Value)>> {
    let res = match client()?.post(format!("{SERVICE_URL}{path}")).json(&body).send().await {
        Ok(res) => res,
        Err(_) => return Ok(None),
    };
    let status = res.status().as_u16();
    let json = res.json().await.unwrap_or(serde_json::Value::Null);
    Ok(Some((status, json)))
}

fn message(json: &serde_json::Value) -> String {
    json["error"].as_str().unwrap_or("Réponse inattendue du service de licences.").to_string()
}

fn machine_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default()
}

#[tauri::command]
pub fn licence_status(licence: State<'_, Licence>) -> Status {
    licence.status(false)
}

/// Ticket courant (en-tête des demandes de mise à jour).
#[tauri::command]
pub fn licence_ticket(licence: State<'_, Licence>) -> Option<String> {
    licence.payload()?;
    licence.lock().ticket.clone()
}

/// Première connexion du poste (ou après une déconnexion).
#[tauri::command]
pub async fn licence_login(licence: State<'_, Licence>, email: String, password: String) -> CmdResult<Status> {
    if licence.machine.is_empty() {
        return Err("Identifiant de cet ordinateur illisible : connexion impossible.".into());
    }
    let body = serde_json::json!({
        "email": email.trim(),
        "password": password,
        "machine_id": licence.machine,
        "machine_name": machine_name(),
        "app_version": env!("CARGO_PKG_VERSION"),
    });
    let Some((status, json)) = post("/api/licence/activate", body).await? else {
        return Err("Service de licences injoignable : vérifiez la connexion internet.".into());
    };
    if status != 200 {
        return Err(message(&json));
    }
    let (Some(ticket), Some(device), Some(secret)) = (json["ticket"].as_str(), json["device_id"].as_str(), json["secret"].as_str()) else {
        return Err(message(&json));
    };
    licence.store(|s| {
        s.ticket = Some(ticket.into());
        s.device_id = Some(device.into());
        s.secret = Some(secret.into());
    });
    if !licence.is_valid() {
        return Err("Licence reçue invalide pour cet ordinateur.".into());
    }
    Ok(licence.status(false))
}

/// Renouvelle le ticket (au lancement, puis régulièrement). Hors ligne, le ticket actuel reste
/// valable jusqu'à son expiration ; compte désactivé ou poste révoqué : accès coupé tout de suite.
#[tauri::command]
pub async fn licence_refresh(licence: State<'_, Licence>) -> CmdResult<Status> {
    let (device, secret) = {
        let s = licence.lock();
        (s.device_id.clone(), s.secret.clone())
    };
    let (Some(device_id), Some(secret)) = (device, secret) else {
        return Ok(licence.status(false));
    };
    let body = serde_json::json!({ "device_id": device_id, "secret": secret, "app_version": env!("CARGO_PKG_VERSION") });
    match post("/api/licence/renew", body).await? {
        None => Ok(licence.status(true)),
        Some((200, json)) => {
            if let Some(ticket) = json["ticket"].as_str() {
                licence.store(|s| s.ticket = Some(ticket.into()));
            }
            Ok(licence.status(false))
        }
        Some((401 | 403, json)) => {
            licence.store(|s| *s = Stored { last_seen: s.last_seen, ..Default::default() });
            Err(message(&json))
        }
        // Service en panne (5xx…) : comme hors ligne.
        Some(_) => Ok(licence.status(true)),
    }
}

/// « Se déconnecter » : libère la place du poste sur le compte.
#[tauri::command]
pub async fn licence_logout(licence: State<'_, Licence>) -> CmdResult<Status> {
    let (device, secret) = {
        let s = licence.lock();
        (s.device_id.clone(), s.secret.clone())
    };
    if let (Some(device_id), Some(secret)) = (device, secret) {
        // Hors ligne : la place sera libérée par l'administrateur (poste à supprimer dans PocketBase).
        let _ = post("/api/licence/deactivate", serde_json::json!({ "device_id": device_id, "secret": secret })).await;
    }
    licence.store(|s| *s = Stored { last_seen: s.last_seen, ..Default::default() });
    Ok(licence.status(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn keypair() -> Ed25519KeyPair {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    fn ticket(key: &Ed25519KeyPair, machine: &str, exp: i64) -> String {
        let payload = serde_json::json!({
            "v": 1, "sub": "u1", "email": "julie@example.fr", "name": "Julie",
            "device": "d1", "machine": machine, "iat": exp - 100, "exp": exp,
        });
        let data = URL_SAFE_NO_PAD.encode(payload.to_string());
        let sig = URL_SAFE_NO_PAD.encode(key.sign(data.as_bytes()));
        format!("{data}.{sig}")
    }

    #[test]
    fn ticket_valid_only_for_this_machine_until_expiry() {
        let key = keypair();
        let public = key.public_key().as_ref();
        let machine = machine_hash("ABC-123");
        let t = ticket(&key, &machine, 1_000);
        assert_eq!(verify(&t, public, &machine, 999).unwrap().email, "julie@example.fr");
        assert!(verify(&t, public, &machine, 1_000).is_none(), "expiré");
        assert!(verify(&t, public, &machine_hash("AUTRE"), 999).is_none(), "autre machine");
        assert!(verify(&t, public, "", 999).is_none(), "machine illisible");
    }

    #[test]
    fn forged_or_damaged_ticket_is_refused() {
        let key = keypair();
        let machine = machine_hash("ABC-123");
        let t = ticket(&key, &machine, 1_000);
        // Signé par une autre clé.
        assert!(verify(&t, keypair().public_key().as_ref(), &machine, 999).is_none());
        // Date prolongée sans signature valide.
        let (_, sig) = t.split_once('.').unwrap();
        let (forged, _) = ticket(&keypair(), &machine, 9_999_999).split_once('.').map(|(d, s)| (d.to_string(), s.to_string())).unwrap();
        assert!(verify(&format!("{forged}.{sig}"), key.public_key().as_ref(), &machine, 999).is_none());
        for bad in ["", "abc", "a.b", "...", "été.été"] {
            assert!(verify(bad, key.public_key().as_ref(), &machine, 999).is_none());
        }
    }

    #[test]
    fn embedded_public_key_is_valid() {
        assert_eq!(STANDARD.decode(PUBLIC_KEY).unwrap().len(), 32);
        // Même empreinte que le service attend : 64 caractères hexadécimaux.
        assert_eq!(machine_hash("x").len(), 64);
    }

    /// Appel réel au service (réseau) : cargo test -- --ignored service_answers
    #[test]
    #[ignore]
    fn service_answers() {
        let body = serde_json::json!({ "email": "inconnu@example.fr", "password": "x", "machine_id": machine_hash("test") });
        let (status, json) = tauri::async_runtime::block_on(post("/api/licence/activate", body)).unwrap().expect("service joignable");
        assert_eq!(status, 401);
        assert_eq!(message(&json), "Email ou mot de passe incorrect.");
    }

    #[test]
    fn clock_set_back_invalidates_the_ticket() {
        let dir = std::env::temp_dir().join(format!("licence-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = keypair();
        let mut licence = Licence::open(&dir);
        licence.public_key = key.public_key().as_ref().to_vec();
        licence.machine = machine_hash("ABC-123");
        let t = ticket(&key, &licence.machine, now() + 15 * 86_400);
        licence.store(|s| s.ticket = Some(t));
        assert!(licence.payload().is_some());
        // Horloge reculée de plus d'un jour après être passée par une date plus récente.
        licence.store(|s| s.last_seen = now() + 2 * 86_400);
        assert!(licence.payload().is_none());
        licence.store(|s| s.last_seen = now() + 3600);
        assert!(licence.payload().is_some(), "petit écart toléré (changement d'heure, fuseau)");
        let _ = std::fs::remove_dir_all(dir);
    }
}
