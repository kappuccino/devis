// Licence du poste : connexion au service de licences, ticket vérifié côté Rust (src-tauri/src/licence.rs).
// Sans licence valide, l'appli n'affiche que l'écran de connexion (les commandes sont refusées).
import { invoke } from "@tauri-apps/api/core";
import { reactive } from "vue";
import { errorMessage } from "./format";

export interface LicenceStatus {
  valid: boolean;
  email: string | null;
  name: string | null;
  /** Fin de validité du ticket (secondes depuis 1970) : fonctionnement hors ligne jusque-là. */
  expires_at: number | null;
  /** Service injoignable lors du dernier renouvellement. */
  offline: boolean;
  /** Développement sans licence (DEVIS_SANS_LICENCE). */
  bypass: boolean;
}

export const licence = reactive({
  /** Premier état lu (avant : écran d'attente). */
  ready: false,
  status: { valid: false, email: null, name: null, expires_at: null, offline: false, bypass: false } as LicenceStatus,
  /** Motif d'un accès coupé (compte désactivé, poste révoqué…), affiché sur l'écran de connexion. */
  error: "",
});

/** Renouvellement du ticket : au lancement puis toutes les 6 heures ; état relu chaque minute. */
const RENEW_EVERY = 6 * 3600_000;
const CHECK_EVERY = 60_000;

export async function startLicence() {
  try {
    // État local d'abord (fonctionne hors ligne), puis renouvellement auprès du service.
    licence.status = await invoke<LicenceStatus>("licence_status");
  } finally {
    licence.ready = true;
  }
  await refreshLicence();
  setInterval(refreshLicence, RENEW_EVERY);
  setInterval(async () => {
    licence.status = await invoke<LicenceStatus>("licence_status");
  }, CHECK_EVERY);
}

export async function refreshLicence() {
  try {
    licence.status = await invoke<LicenceStatus>("licence_refresh");
  } catch (e) {
    licence.error = errorMessage(e);
    licence.status = await invoke<LicenceStatus>("licence_status");
  }
}

export async function login(email: string, password: string) {
  licence.status = await invoke<LicenceStatus>("licence_login", { email, password });
  licence.error = "";
}

export async function logout() {
  licence.status = await invoke<LicenceStatus>("licence_logout");
}

/** Ticket courant, pour l'en-tête des demandes de mise à jour. */
export const licenceTicket = () => invoke<string | null>("licence_ticket");
