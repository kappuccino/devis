import { invoke } from "@tauri-apps/api/core";

export type Family = "CFA" | "CFO";

export interface Product {
  ref: string;
  enedis_code: string | null;
  designation: string;
  public_price: number;
  eco_tax: number | null;
  eco_tax_code: string | null;
  family: Family | null;
  threshold_price: number | null;
}

export interface ProductHit {
  ref: string;
  designation: string;
  enedis_code: string | null;
}

export interface Client {
  code: string;
  name: string;
  group_name: string | null;
  subgroup: string | null;
  sales_rep: string | null;
  discount_cfa: number;
  discount_cfo: number;
  email: string | null;
  franco: string | null;
  siren: string | null;
  price_lists: string[];
}

/** Conditions de prix d'un devis : copiées du client à sa sélection, modifiables sur le devis. */
export interface PricingContext {
  price_lists: string[];
  discount_cfa: number;
  discount_cfo: number;
  /** Liste forcée (parmi les favorites), prioritaire sur les autres ; null si aucune. */
  forced_price_list: string | null;
}

export interface PriceList {
  code: string;
  label: string | null;
  item_count: number;
  client_count: number;
}

export interface PriceListItem {
  product_ref: string;
  designation: string | null;
  price: number;
  label: string | null;
  public_price: number | null;
  family: Family | null;
}

/** Nombre et montant des devis, dont affaires obtenues. */
export interface StatCounts {
  count: number;
  total: number;
  won_count: number;
  won_total: number;
}

export interface StatGroup extends StatCounts {
  label: string;
  code: string;
}

/** Statistiques des devis (montants HT après remise globale, frais compris ; options exclues). */
export interface QuoteStats {
  years: string[];
  count: number;
  total: number;
  clients: number;
  /** Affaires obtenues (coche de la liste des devis). */
  won_count: number;
  won_total: number;
  /** month : « AAAA-MM ». */
  by_month: ({ month: string } & StatCounts)[];
  /** code vide : client ponctuel (ou commercial). */
  top_clients: StatGroup[];
  by_sales_rep: StatGroup[];
  top_products: { product_ref: string; designation: string; quantity: number; total: number; quotes: number }[];
}

/** Une référence dans un devis (écran « Évolution du chiffrage »). */
export interface HistoryLine {
  quote_id: number;
  number: string;
  date: string;
  client_code: string;
  client_name: string;
  sales_rep: string;
  quantity: number;
  unit_price: number;
  /** Remise de la ligne et remise globale du devis (%). */
  discount: number;
  quote_discount: number;
  /** Prix réellement devisé : remise de ligne puis remise globale (sauf option). */
  net_unit_price: number;
  public_price: number | null;
  lpn_price: number | null;
  is_option: boolean;
}

export interface ProductHistory {
  /** null : référence absente du catalogue actuel. */
  product: {
    product_ref: string;
    designation: string;
    enedis_code: string | null;
    public_price: number;
    threshold_price: number | null;
    family: Family | null;
  } | null;
  /** Du plus ancien au plus récent. */
  lines: HistoryLine[];
}

/** Copie de la base des devis (dossier de sauvegarde). */
export interface BackupFile {
  path: string;
  name: string;
  size: number;
  /** Date de la copie (ms). */
  modified: number | null;
  /** Copie de sécurité faite avant une restauration (jamais supprimée automatiquement). */
  safety: boolean;
}

export interface BackupInfo {
  dir: string;
  default_dir: string;
  files: BackupFile[];
}

export interface ClientRef {
  code: string;
  name: string;
}

export interface ResolvedPrice {
  product_ref: string;
  /** Prix public − remise CFA / CFO (produit du catalogue). */
  discounted_price: number | null;
  /** Prix de la liste de prix (LPN) retenue, et son code. */
  lpn_price: number | null;
  lpn_list: string | null;
  enedis_code: string | null;
  designation: string;
  unit_price: number;
  source: string;
  family: Family | null;
  public_price: number | null;
  threshold_price: number | null;
}

export interface QuoteSummary {
  id: number;
  number: string;
  /** null : client ponctuel (n'existe que dans le devis). */
  client_code: string | null;
  client_name: string;
  date: string;
  total_ht: number;
  /** Total après la remise globale. */
  total_net: number;
  line_count: number;
  /** Affaire obtenue : date du marquage (AAAA-MM-JJ), null sinon. */
  won_at: string | null;
  /** N° de version (1 : original) ; vrai si une version plus récente existe. */
  version: number;
  superseded: boolean;
  /** Nom de l'affaire. */
  project_name: string;
}

/**
 * `item` : article ; `text` : texte libre (dans `designation`) ;
 * `subtotal` : sous-total des articles depuis le sous-total précédent (libellé dans `designation`).
 */
export interface QuoteLine {
  /** `shipping` / `billing` : frais de port / de facturation (montant HT dans unit_price). */
  kind: "item" | "title" | "text" | "subtotal" | "shipping" | "billing";
  product_ref: string;
  /** Code ENEDIS, copié du catalogue à la saisie de la ligne. */
  enedis_code: string | null;
  designation: string;
  /** Vide (null) tant qu'elle n'a pas été saisie ; enregistrée à 0. */
  quantity: number | null;
  unit_price: number;
  /** Remise supplémentaire de la ligne, en %. */
  discount: number;
  /** Ligne en option : hors total HT, comptée dans « Total options ». */
  is_option: boolean;
  price_source: string | null;
  public_price: number | null;
  /** Affiché dans l'appli, jamais sur le PDF. */
  threshold_price: number | null;
  /** Affichage seulement (jamais sur le PDF) : prix public remisé et prix LPN au moment du devis. */
  discounted_price: number | null;
  lpn_price: number | null;
  lpn_list: string | null;
}

/** Contact chez le client et commercial du devis (imprimés sur le PDF). */
export interface QuoteContact {
  contact_name: string;
  contact_email: string;
  contact_phone: string;
  sales_rep: string;
}

export interface Quote extends PricingContext, QuoteContact {
  id: number | null;
  number: string | null;
  /** null : client ponctuel (n'existe que dans le devis). */
  client_code: string | null;
  client_name: string;
  date: string;
  notes: string | null;
  /** Somme des lignes (remises de ligne comprises). */
  total_ht: number;
  /** Remise globale sur le total, en %. */
  discount_pct: number;
  /** Total après la remise globale. */
  total_net: number;
  /** Total des lignes en option (hors total HT). */
  total_options: number;
  lines: QuoteLine[];
  /** N° de version (1 : original) et id de l'original (null pour lui). */
  version?: number;
  version_of?: number | null;
  /** Toutes les versions du devis (lecture seule). */
  versions?: QuoteVersion[];
  /** Affaire obtenue : date du marquage (lecture seule, voir setQuoteWon). */
  won_at?: string | null;
  /** Devis type (modèle, sans client ni prix) et son nom. */
  is_template?: boolean;
  template_name?: string;
  /** Nom de l'affaire (chantier, opération…), imprimé sur le PDF. */
  project_name?: string;
}

/** Devis type, dans la liste de l'onglet « Devis types ». */
export interface QuoteTemplate {
  id: number;
  name: string;
  /** Nombre d'articles. */
  line_count: number;
  notes: string | null;
  updated_at: string;
}

/** Une version d'un devis. */
export interface QuoteVersion {
  id: number;
  number: string;
  version: number;
  date: string;
  won_at: string | null;
}

/** Document joint au PDF « Devis + Docs » (choix mémorisé avec le devis). */
export interface QuoteAttachment {
  /** `index` : trouvé dans la documentation ; `external` : fichier ajouté à la main. */
  source: "index" | "external";
  /** Produit du devis auquel le document est rattaché ('' pour un document ajouté). */
  product_ref: string;
  path: string;
  /** Page de la documentation ; null : fichier entier. */
  page_num: number | null;
  /** Coché (inclus dans le PDF) ou écarté. */
  included: boolean;
}

export interface ImportReport {
  products: number;
  clients: number;
  price_lists: number;
  price_list_items: number;
  client_links: number;
  warnings: string[];
}

export interface DbStats {
  path: string;
  products: number;
  clients: number;
  price_lists: number;
  price_list_items: number;
  quotes: number;
}

export type Settings = Record<string, string>;

export const api = {
  listProducts: () => invoke<Product[]>("list_products"),
  searchProducts: (query: string, priceLists: string[]) =>
    invoke<ProductHit[]>("search_products", { query, priceLists }),

  listClients: () => invoke<Client[]>("list_clients"),
  getClient: (code: string) => invoke<Client | null>("get_client", { code }),
  updateClient: (code: string, discountCfa: number, discountCfo: number, priceLists: string[]) =>
    invoke<void>("update_client", { code, discountCfa, discountCfo, priceLists }),

  setClientPriceList: (clientCode: string, priceListCode: string, attached: boolean) =>
    invoke<void>("set_client_price_list", { clientCode, priceListCode, attached }),
  /** Lignes de devis d'une référence, pour l'écran « Évolution du chiffrage ». */
  productPriceHistory: (productRef: string) => invoke<ProductHistory>("product_price_history", { productRef }),
  /** Statistiques d'une année (« 2026 ») ou de toute la base (null). */
  quoteStats: (year: string | null) => invoke<QuoteStats>("quote_stats", { year }),
  backupInfo: () => invoke<BackupInfo>("backup_info"),
  backupNow: () => invoke<string>("backup_now"),
  /** Renvoie le chemin de la copie de sécurité de la base remplacée. */
  restoreBackup: (path: string) => invoke<string>("restore_backup", { path }),
  /** Export Excel d'une liste de prix d'un client (gabarit des réglages). */
  exportPriceList: (
    path: string,
    clientCode: string,
    priceListCode: string,
    contact: { lastName: string; firstName: string; email: string },
  ) => invoke<void>("export_price_list", { path, clientCode, priceListCode, contact }),
  listPriceLists: () => invoke<PriceList[]>("list_price_lists"),
  getPriceListItems: (code: string) => invoke<PriceListItem[]>("get_price_list_items", { code }),
  getPriceListClients: (code: string) => invoke<ClientRef[]>("get_price_list_clients", { code }),

  resolvePrice: (pricing: PricingContext, productRef: string) =>
    invoke<ResolvedPrice>("resolve_price", { pricing, productRef }),

  listQuotes: () => invoke<QuoteSummary[]>("list_quotes"),
  getQuote: (id: number) => invoke<Quote>("get_quote", { id }),
  saveQuote: (quote: Quote) => invoke<Quote>("save_quote", { quote }),
  deleteQuote: (id: number) => invoke<void>("delete_quote", { id }),
  duplicateQuote: (id: number, date: string) => invoke<Quote>("duplicate_quote", { id, date }),
  listQuoteTemplates: () => invoke<QuoteTemplate[]>("list_quote_templates"),
  /** Enregistre un devis comme devis type (lignes et notes, sans client ni prix). */
  saveAsTemplate: (id: number, name: string, date: string) => invoke<Quote>("save_as_template", { id, name, date }),
  /** Nouvelle version (numéro de l'original suivi de « -V2 »…), datée de `date`. */
  newQuoteVersion: (id: number, date: string) => invoke<Quote>("new_quote_version", { id, date }),
  /** Renvoie la date du marquage (null : affaire remise en attente). */
  setQuoteWon: (id: number, won: boolean) => invoke<string | null>("set_quote_won", { id, won }),

  getQuoteAttachments: (quoteId: number) => invoke<QuoteAttachment[]>("get_quote_attachments", { quoteId }),
  saveQuoteAttachments: (quoteId: number, attachments: QuoteAttachment[]) =>
    invoke<void>("save_quote_attachments", { quoteId, attachments }),
  filesExist: (paths: string[]) => invoke<boolean[]>("files_exist", { paths }),

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  dbStats: () => invoke<DbStats>("db_stats"),
  importLpn: (path: string) => invoke<ImportReport>("import_lpn", { path }),
  saveFile: (path: string, contents: Uint8Array) =>
    invoke<void>("save_file", { path, contents: Array.from(contents) }),
};
