<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, onUpdated, ref, watch } from "vue";
import { useRouter } from "vue-router";
import AutoComplete, { type AutoCompleteCompleteEvent } from "primevue/autocomplete";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import Button from "primevue/button";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import Dialog from "primevue/dialog";
import InputNumber from "primevue/inputnumber";
import Select from "primevue/select";
import Checkbox from "primevue/checkbox";
import {
  api,
  type Client,
  type PriceList,
  type PricingContext,
  type ProductHit,
  type Quote,
  type QuoteContact,
  type QuoteLine,
  type QuoteVersion,
} from "../api";
import { errorMessage, formatDate, formatEuro, formatUnitPrice, MOD, round2, todayIso } from "../format";
import { exportQuotePdf } from "../composables/usePdf";
import QuoteDocsDialog from "../components/docs/QuoteDocsDialog.vue";
import PriceListDrawer from "../components/PriceListDrawer.vue";
import { readDraft, removeDraft, writeDraft, type QuoteDraft } from "../drafts";
import { useConfirm } from "primevue/useconfirm";
import { useLineClipboard } from "../composables/useLineClipboard";
import {
  FEE_KINDS,
  cleanLines,
  insertBlock,
  isFee,
  lineTotal,
  moveBlockBefore,
  netUnitPrice,
  quoteTotals,
  subtotals,
  type FeeKind,
} from "../quoteLines";
import { favoriteLists } from "../favorites";
import { lastSalesRep, rememberSalesRep, salesRepList } from "../salesReps";
import { applyColumnTabOrder } from "../gridNav";
import { autoFeeChanges, feeRulesFrom, type FeeRules } from "../fees";

const props = defineProps<{ id?: string }>();
const router = useRouter();
const toast = useToast();
const confirm = useConfirm();

/** Ligne en cours d'édition : la ligne enregistrée + ce qui sert seulement à l'affichage. */
interface EditLine extends QuoteLine {
  key: number;
  resolvedRef: string | null;
  /** Code ENEDIS du produit trouvé (pour savoir si la colonne ENEDIS a été modifiée). */
  resolvedEnedis?: string | null;
  /** Réf en cours de résolution : évite un double appel (sélection + Entrée + blur). */
  pendingRef: string | null;
  error: string | null;
  /**
   * Texte des champs quantité, prix et remise, tel que tapé. `quantity` / `unit_price` / `discount`
   * en sont déduits à chaque touche : totaux et sous-totaux suivent la frappe, sans réécrire le champ.
   */
  qtyText: string;
  priceText: string;
  discountText: string;
  /** Frais ajouté automatiquement (conditions de vente), tant qu'on ne l'a pas modifié. */
  auto?: boolean;
}

let nextKey = 0;
const blankLine = (): EditLine => ({
  key: nextKey++,
  kind: "item",
  product_ref: "",
  designation: "",
  quantity: null,
  unit_price: 0,
  discount: 0,
  is_option: false,
  enedis_code: null,
  price_source: null,
  public_price: null,
  threshold_price: null,
  discounted_price: null,
  lpn_price: null,
  lpn_list: null,
  resolvedRef: null,
  pendingRef: null,
  error: null,
  qtyText: "",
  priceText: "",
  discountText: "",
});

/** « 1 234,5 » → 1234.5 ; vide ou illisible → null. */
function parseNumber(text: string): number | null {
  const t = text.replace(/[\s\u00a0\u202f€]/g, "").replace(",", ".");
  if (!t) return null;
  const v = Number(t);
  return Number.isFinite(v) ? v : null;
}

/** Valeur affichée dans un champ en cours d'édition (sans séparateur de milliers). */
const editText = (v: number | null, decimals: 2 | 3) =>
  v == null
    ? ""
    : v.toLocaleString("fr-FR", {
        useGrouping: false,
        minimumFractionDigits: decimals === 2 ? 2 : 0,
        maximumFractionDigits: decimals,
      });

/** Ligne chargée (devis, collage) : on prépare le texte de ses champs. */
const withTexts = <T extends QuoteLine>(l: T) => ({
  ...l,
  // Le code ENEDIS affiché correspond déjà au produit : le quitter ne relance pas de recherche.
  resolvedEnedis: l.enedis_code,
  qtyText: editText(l.quantity, 3),
  priceText: editText(l.unit_price, 2),
  discountText: l.discount ? editText(l.discount, 3) : "",
});

function onQtyInput(line: EditLine, text: string) {
  line.qtyText = text;
  line.quantity = parseNumber(text);
}

function onPriceInput(line: EditLine, text: string) {
  line.priceText = text;
  line.unit_price = parseNumber(text) ?? 0;
}

/** Remise supplémentaire de la ligne, en % (0 à 100). */
function onDiscountInput(line: EditLine, text: string) {
  line.discountText = text;
  line.discount = Math.min(100, Math.max(0, parseNumber(text) ?? 0));
}

function onPriceBlur(line: EditLine) {
  line.unit_price = round2(line.unit_price);
  line.priceText = editText(line.unit_price, 2);
}

const quoteId = ref<number | null>(null);
const number = ref<string | null>(null);
/** Toutes les versions du devis enregistré (une seule : pas de versions). */
const versions = ref<QuoteVersion[]>([]);
const date = ref(todayIso());
const notes = ref("");
const lines = ref<EditLine[]>([blankLine()]);

const clients = ref<Client[]>([]);
const allPriceLists = ref<PriceList[]>([]);
/** Client de la base choisi dans la recherche (null : aucun, ou client ponctuel). */
const client = ref<Client | null>(null);
const clientSuggestions = ref<Client[]>([]);

// Client et conditions de prix du devis : copiés du client à sa sélection, puis modifiables
// sur ce devis seulement (la fiche client n'est pas touchée). Code null : client ponctuel.
const clientCode = ref<string | null>(null);
const clientName = ref("");
const discountCfa = ref(0);
const discountCfo = ref(0);
const priceLists = ref<string[]>([]);
/** Liste de prix forcée sur ce devis (parmi les favorites) : prioritaire sur les autres. */
const forcedList = ref<string | null>(null);
const hasClient = computed(() => clientName.value.trim() !== "");
const pricing = computed<PricingContext>(() => ({
  price_lists: [...priceLists.value],
  discount_cfa: discountCfa.value ?? 0,
  discount_cfo: discountCfo.value ?? 0,
  forced_price_list: forcedList.value,
}));
/** Toutes les listes où chercher un produit (forcée comprise). */
const searchLists = computed(() => [...priceLists.value, ...(forcedList.value ? [forcedList.value] : [])]);

// Listes favorites (Réglages) proposées pour forcer une liste ; la liste forcée actuelle reste
// proposée même si elle a été retirée des favorites depuis.
const favorites = ref<string[]>([]);
const forcedOptions = computed(() => {
  const codes = [...favorites.value];
  if (forcedList.value && !codes.includes(forcedList.value)) codes.push(forcedList.value);
  return codes.map((code) => {
    const label = allPriceLists.value.find((l) => l.code === code)?.label;
    return { code, label: label ? `${code} — ${label}` : code };
  });
});

function onForcedListChange() {
  repriceLines();
}
const pricingKey = computed(() => JSON.stringify(pricing.value));
const productSuggestions = ref<ProductHit[]>([]);
const saving = ref(false);

// Frais de port / de facturation : à part des autres lignes, toujours en bas du devis
// (pas de déplacement ni de sélection), un seul de chaque.
const feeLines = ref<EditLine[]>([]);
const FEE_LABELS: Record<FeeKind, string> = { shipping: "Frais de port", billing: "Frais de facturation" };
const hasFee = (kind: FeeKind) => feeLines.value.some((l) => l.kind === kind);

function addFee(kind: FeeKind, auto?: { amount: number }) {
  if (hasFee(kind)) return;
  const fee: EditLine = { ...blankLine(), kind, quantity: 1, designation: FEE_LABELS[kind] };
  if (auto) Object.assign(fee, { auto: true, unit_price: auto.amount, priceText: editText(auto.amount, 2) });
  // Ordre fixe : frais de port, puis frais de facturation.
  feeLines.value = [...feeLines.value, fee].sort((a, b) => FEE_KINDS.indexOf(a.kind as FeeKind) - FEE_KINDS.indexOf(b.kind as FeeKind));
  if (!auto) nextTick(() => document.getElementById(`fee-${fee.key}`)?.focus());
}

function removeFee(fee: EditLine) {
  feeLines.value = feeLines.value.filter((l) => l !== fee);
  // Retiré à la main : on ne le repropose plus sur ce devis.
  dismissedFees.add(fee.kind as FeeKind);
}

// Frais automatiques (seuils dans Réglages → Config PDF) : ajoutés / retirés quand le total des
// produits passe sous / au-dessus du seuil. Un frais modifié ou retiré à la main n'est plus touché.
const feeRules = ref<FeeRules>({});
const dismissedFees = new Set<FeeKind>();

function applyAutoFees() {
  const present = feeLines.value.map((l) => ({ kind: l.kind as FeeKind, auto: !!l.auto }));
  const { add, remove } = autoFeeChanges(totals.value.products, feeRules.value, present, dismissedFees);
  if (remove.length) feeLines.value = feeLines.value.filter((l) => !remove.includes(l.kind as FeeKind));
  for (const kind of add) addFee(kind, { amount: feeRules.value[kind]!.amount });
}

/** Remise globale sur les produits, en % (les frais ne sont pas remisés). */
const globalDiscount = ref(0);
/** Total HT (produits hors options + frais), remise, total remisé et total des options. */
const totals = computed(() => quoteTotals([...lines.value, ...feeLines.value], globalDiscount.value));
// Frais automatiques : seulement sur les modifications du devis, pas à son chargement.
watch(
  () => totals.value.products,
  () => {
    if (draftReady.value) applyAutoFees();
  },
);
const total = computed(() => totals.value.total);
const netTotal = computed(() => totals.value.net);
const filledLines = computed(() => lines.value.filter((l) => l.kind === "item" && l.product_ref.trim()));
/** Montant de chaque sous-total, par clé de ligne (recalculé à chaque frappe et déplacement). */
const subtotalAmounts = computed(() => {
  const amounts = subtotals(lines.value);
  return new Map(lines.value.map((l) => [l.key, amounts.get(l)]));
});

/**
 * Navigation entre les quantités, lignes de texte et sous-totaux sautés :
 * - Entrée / ↓ : quantité de l'article suivant ; après le dernier, Entrée revient à la saisie de réf ;
 * - ↑ : quantité de l'article précédent.
 */
function moveToQuantity(line: EditLine, direction: 1 | -1, e?: Event) {
  e?.preventDefault();
  line.qtyText = editText(line.quantity, 3);
  const items = lines.value.filter((l) => l.kind === "item" && !isEntryLine(l));
  const target = items[items.indexOf(line) + direction];
  let input = target ? document.getElementById(`qty-${target.key}`) : null;
  if (!target && direction === 1 && e instanceof KeyboardEvent && e.key === "Enter") {
    input = document.getElementById(`ref-${lines.value[lines.value.length - 1].key}`);
  }
  if (input instanceof HTMLInputElement) {
    input.focus();
    input.select();
  }
}
const isBlankItem = (l: EditLine) => l.kind === "item" && !l.product_ref;
/** La dernière ligne est la ligne de saisie : ni déplaçable, ni enregistrée tant qu'elle est vide. */
const isEntryLine = (l: EditLine) => l === lines.value[lines.value.length - 1] && isBlankItem(l);

/** Tab dans la recherche client sans aucune correspondance : on crée un client ponctuel avec ce nom. */
function onClientTab(e: KeyboardEvent) {
  const typed = (e.target as HTMLInputElement).value.trim();
  if (!typed || typed === client.value?.name) return;
  const q = typed.toLowerCase();
  if (clients.value.some((c) => c.name.toLowerCase().includes(q) || c.code.toLowerCase().includes(q))) return;
  e.preventDefault();
  openEphemeral(typed);
}

/** Choix d'un client de la base : on copie ses conditions dans le devis et on recalcule les prix. */
/** Contact chez le client et commercial : propres au devis. */
const emptyContact = (): QuoteContact => ({ contact_name: "", contact_email: "", contact_phone: "", sales_rep: "" });
const contact = ref<QuoteContact>(emptyContact());

async function applyClient(c: Client) {
  clientCode.value = c.code;
  clientName.value = c.name;
  // Nouveau client : contact vide ; le commercial choisi reste (le nom de la fiche client est incomplet).
  contact.value = { ...emptyContact(), sales_rep: contact.value.sales_rep };
  discountCfa.value = c.discount_cfa;
  discountCfo.value = c.discount_cfo;
  priceLists.value = [...c.price_lists];
  await repriceLines();
}

// Client ponctuel : n'existe que dans ce devis (nom, remises, une liste de prix au plus).
const lastClientQuery = ref("");
const ephemeralVisible = ref(false);
const ephemeral = ref({ name: "", cfa: 0, cfo: 0, list: null as string | null });
const priceListOptions = computed(() =>
  allPriceLists.value.map((l) => ({ code: l.code, label: `${l.code}${l.label ? ` — ${l.label}` : ""}` })),
);

function openEphemeral(name?: string) {
  ephemeral.value = {
    name: name ?? (clientCode.value ? lastClientQuery.value : clientName.value || lastClientQuery.value),
    cfa: discountCfa.value,
    cfo: discountCfo.value,
    list: !clientCode.value ? (priceLists.value[0] ?? null) : null,
  };
  ephemeralVisible.value = true;
}

async function applyEphemeral() {
  const { name, cfa, cfo, list } = ephemeral.value;
  if (!name.trim()) {
    toast.add({ severity: "warn", summary: "Indiquez la raison sociale", life: 2500 });
    return;
  }
  ephemeralVisible.value = false;
  if (clientCode.value) contact.value = { ...emptyContact(), sales_rep: contact.value.sales_rep };
  client.value = null;
  clientCode.value = null;
  clientName.value = name.trim();
  discountCfa.value = cfa ?? 0;
  discountCfo.value = cfo ?? 0;
  priceLists.value = list ? [list] : [];
  await repriceLines();
}

/** Liste de prix affichée dans le tiroir latéral (null : fermé). */
const drawerList = ref<string | null>(null);
const openPriceList = (code: string) => (drawerList.value = code);

/** Retire le client du devis (conditions et contact compris) ; les prix sont recalculés. */
async function clearClient() {
  client.value = null;
  clientCode.value = null;
  clientName.value = "";
  discountCfa.value = 0;
  discountCfo.value = 0;
  priceLists.value = [];
  contact.value = { ...emptyContact(), sales_rep: contact.value.sales_rep };
  await repriceLines();
}

// Totaux : la colonne des montants se cale sur la colonne « Total HT » du tableau
// (même largeur, même distance au bord droit), quelle que soit la largeur de la fenêtre.
const linesTable = ref<HTMLTableElement | null>(null);
const totalsPanel = ref<HTMLElement | null>(null);
const totalHeader = ref<HTMLTableCellElement | null>(null);
const totalColumn = ref({ width: 100, tail: 150 });
function measureTotalColumn() {
  const table = linesTable.value?.getBoundingClientRect();
  const th = totalHeader.value?.getBoundingClientRect();
  if (!table || !th || !th.width) return;
  totalColumn.value = { width: th.width, tail: table.right - th.right };
}
// Colonnes du panneau : libellé | saisie | montant | croix des frais. Le panneau s'arrête après la
// croix ; une marge à droite le cale pour que le bord droit des montants tombe sous « Total HT ».
// La colonne des montants peut déborder à gauche de « Total HT » : seul son bord droit compte.
const TOTALS_GAP = 12;
const TOTALS_REMOVE = 28;
const TOTALS_PADDING = 12;
const totalsStyle = computed(() => ({
  gridTemplateColumns: `minmax(170px, auto) 110px ${Math.max(totalColumn.value.width - TOTALS_GAP, 130)}px ${TOTALS_REMOVE}px`,
  marginRight: `${Math.max(totalColumn.value.tail - TOTALS_GAP - TOTALS_REMOVE - TOTALS_PADDING, 0)}px`,
}));
let tableObserver: ResizeObserver | null = null;
onMounted(() => {
  tableObserver = new ResizeObserver(measureTotalColumn);
  if (linesTable.value) tableObserver.observe(linesTable.value);
  measureTotalColumn();
});
onBeforeUnmount(() => tableObserver?.disconnect());

// Tab descend dans la colonne, Maj+Tab remonte (tabindex recalculés à chaque rendu).
const updateTabOrder = () => applyColumnTabOrder(linesTable.value, totalsPanel.value);
onMounted(updateTabOrder);
onUpdated(updateTabOrder);

// Commercial : choisi dans la liste des Réglages ; le dernier choisi est repris sur les nouveaux devis.
const salesReps = ref<string[]>([]);
const salesRepOptions = computed(() => {
  const current = contact.value.sales_rep;
  return current && !salesReps.value.includes(current) ? [...salesReps.value, current] : salesReps.value;
});

/** Liste de prix unique d'un client ponctuel, modifiable dans l'en-tête du devis. */
const ephemeralList = computed({
  get: () => priceLists.value[0] ?? null,
  set: (code: string | null) => {
    priceLists.value = code ? [code] : [];
    repriceLines();
  },
});

function searchClients(e: AutoCompleteCompleteEvent) {
  lastClientQuery.value = e.query.trim();
  const q = e.query.trim().toLowerCase();
  clientSuggestions.value = clients.value
    .filter((c) => c.name.toLowerCase().includes(q) || c.code.toLowerCase().includes(q))
    .slice(0, 50);
}

/**
 * La recherche part 200 ms après la frappe : si la réf a déjà été validée (Entrée) ou si le curseur
 * est passé ailleurs, on n'ouvre pas la liste (une liste vide la referme, cf. show-empty-message).
 */
const isStale = (line: EditLine) =>
  line.pendingRef != null ||
  line.product_ref.trim() === line.resolvedRef ||
  document.activeElement?.id !== `ref-${line.key}`;

async function searchProducts(e: AutoCompleteCompleteEvent, line: EditLine) {
  if (isStale(line)) {
    productSuggestions.value = [];
    return;
  }
  try {
    const hits = await api.searchProducts(e.query, searchLists.value);
    productSuggestions.value = isStale(line) ? [] : hits;
  } catch {
    productSuggestions.value = [];
  }
}

/** Colonne ENEDIS : mêmes propositions que la référence, limitées aux produits qui ont un code ENEDIS. */
const enedisSuggestions = ref<ProductHit[]>([]);
const isStaleEnedis = (line: EditLine) =>
  line.pendingRef != null || document.activeElement?.id !== `enedis-${line.key}`;
async function searchEnedis(e: AutoCompleteCompleteEvent, line: EditLine) {
  if (isStaleEnedis(line)) {
    enedisSuggestions.value = [];
    return;
  }
  try {
    const hits = (await api.searchProducts(e.query, searchLists.value)).filter((h) => h.enedis_code);
    enedisSuggestions.value = isStaleEnedis(line) ? [] : hits;
  } catch {
    enedisSuggestions.value = [];
  }
}

/**
 * Tab juste après avoir tapé une référence ou un code ENEDIS : on valide la ligne et on passe
 * à sa quantité (au lieu de descendre la colonne, voir gridNav). Maj+Tab garde l'ordre normal.
 */
async function validateAndGoToQuantity(line: EditLine, field: "ref" | "enedis", e: KeyboardEvent) {
  if (e.shiftKey) return;
  const value = field === "ref" ? line.product_ref.trim() : (line.enedis_code ?? "").trim();
  const known = field === "ref" ? line.resolvedRef : line.resolvedRef != null ? line.resolvedEnedis : null;
  if (!value || value === known) return;
  e.preventDefault();
  if (field === "enedis") line.product_ref = value;
  await resolve(line, false, field);
  if (line.error) return;
  await nextTick();
  const qty = document.getElementById(`qty-${line.key}`);
  if (qty instanceof HTMLInputElement) {
    qty.focus();
    qty.select();
  }
}

/** La dernière ligne est toujours vide : on y place le curseur pour enchaîner la saisie. */
/** `field` : on reste dans la colonne utilisée pour saisir (référence ou code ENEDIS). */
async function focusNewLine(field: "ref" | "enedis" = "ref") {
  const last = lines.value[lines.value.length - 1];
  await nextTick();
  document.getElementById(`${field}-${last.key}`)?.focus();
}

async function resolve(line: EditLine, focusNext = false, field: "ref" | "enedis" = "ref") {
  const r = line.product_ref.trim();
  if (r && r === line.pendingRef) return;
  line.product_ref = r;
  if (!r) {
    Object.assign(line, {
      designation: "",
      unit_price: 0,
      priceText: "",
      price_source: null,
      resolvedRef: null,
      public_price: null,
      threshold_price: null,
      discounted_price: null,
      lpn_price: null,
      lpn_list: null,
      error: null,
    });
    return;
  }
  if (!hasClient.value) {
    line.error = "Choisissez d'abord un client";
    return;
  }
  line.pendingRef = r;
  try {
    const p = await api.resolvePrice(pricing.value, r);
    Object.assign(line, {
      product_ref: p.product_ref,
      enedis_code: p.enedis_code,
      designation: p.designation,
      unit_price: p.unit_price,
      priceText: editText(p.unit_price, 2),
      price_source: p.source,
      public_price: p.public_price,
      threshold_price: p.threshold_price,
      discounted_price: p.discounted_price,
      lpn_price: p.lpn_price,
      lpn_list: p.lpn_list,
      resolvedRef: p.product_ref,
      resolvedEnedis: p.enedis_code,
      error: null,
    });
  } catch (e) {
    Object.assign(line, {
      resolvedRef: r,
      price_source: null,
      public_price: null,
      threshold_price: null,
      discounted_price: null,
      lpn_price: null,
      lpn_list: null,
      error: errorMessage(e),
    });
  } finally {
    line.pendingRef = null;
  }
  if (lines.value[lines.value.length - 1] === line) lines.value.push(blankLine());
  // En cas d'erreur, le curseur reste sur la ligne à corriger.
  if (focusNext && !line.error) focusNewLine(field);
}

function onRefBlur(line: EditLine) {
  if (line.product_ref.trim() !== line.resolvedRef) resolve(line);
}

/** Choix dans le sélecteur ou Entrée : on valide la réf puis on passe à une nouvelle ligne. */
function commitRef(line: EditLine) {
  if (line.product_ref.trim() !== line.resolvedRef) resolve(line, true);
  else if (line.product_ref && !line.error) focusNewLine();
}

/**
 * Saisie par code ENEDIS (colonne ENEDIS) : le serveur retrouve le produit qui porte ce code
 * (avec ou sans les points). Entrée valide et passe à la colonne ENEDIS d'une nouvelle ligne.
 */
function commitEnedis(line: EditLine, focusNext: boolean) {
  const code = (line.enedis_code ?? "").trim();
  if (!code) return;
  const unchanged = line.resolvedRef != null && code === line.resolvedEnedis;
  if (unchanged) {
    if (focusNext && !line.error) focusNewLine("enedis");
    return;
  }
  line.product_ref = code;
  resolve(line, focusNext, "enedis");
}

/**
 * Recalcule les prix avec les conditions du devis.
 * `onlyPublic` : seulement les lignes au prix public remisé (après un changement de remise),
 * pour ne pas écraser les prix de liste ni les prix saisis à la main.
 */
async function repriceLines(onlyPublic = false) {
  const priced = filledLines.value.filter((l) => !onlyPublic || l.price_source?.startsWith("Public"));
  if (!priced.length) return;
  // La réf est déjà connue : on force un nouveau calcul.
  priced.forEach((l) => (l.resolvedRef = null));
  await Promise.all(priced.map((l) => resolve(l)));
  toast.add({ severity: "info", summary: "Prix recalculés", life: 2000 });
}

/** Ajoute un titre, une ligne de texte ou un sous-total en fin de devis ; on la déplace ensuite à la poignée. */
function insertSpecialLine(kind: "title" | "text" | "subtotal") {
  const added: EditLine = { ...blankLine(), kind, quantity: 0, designation: kind === "subtotal" ? "Sous-total" : "" };
  lines.value = insertBlock(lines.value, [added], null, entryLine());
  if (!entryLine()) lines.value.push(blankLine());
  if (kind !== "subtotal") nextTick(() => document.getElementById(`text-${added.key}`)?.focus());
}

// ---------- Glisser-déposer ----------
// Fait main (pointeur + position des lignes) : les bibliothèques testées géraient mal les lignes
// de tableau dans la fenêtre de l'appli. Si la ligne attrapée est sélectionnée, toute la sélection suit.

const tbodyEl = ref<HTMLElement>();
const drag = ref<{ keys: Set<number>; startY: number; moved: boolean; targetKey: number | null } | null>(null);

function startDrag(line: EditLine, e: PointerEvent) {
  if (e.button !== 0) return;
  e.preventDefault();
  (document.activeElement as HTMLElement | null)?.blur?.();
  const block = selectedKeys.value.has(line.key) ? selectionBlock() : [line];
  drag.value = { keys: new Set(block.map((l) => l.key)), startY: e.clientY, moved: false, targetKey: null };
  document.body.classList.add("dragging-lines");
  window.addEventListener("pointermove", onDragMove);
  window.addEventListener("pointerup", endDrag);
  window.addEventListener("pointercancel", stopDrag);
}

/** Ligne devant laquelle on lâcherait : la première dont le milieu est sous le pointeur (null = fin). */
function onDragMove(e: PointerEvent) {
  const d = drag.value;
  if (!d || !tbodyEl.value) return;
  if (!d.moved && Math.abs(e.clientY - d.startY) < 4) return;
  d.moved = true;
  const rows = Array.from(tbodyEl.value.querySelectorAll<HTMLElement>("tr[data-key]"));
  const row = rows.find((r) => {
    const box = r.getBoundingClientRect();
    return e.clientY < box.top + box.height / 2;
  });
  d.targetKey = row ? Number(row.dataset.key) : null;
  // Défilement automatique près des bords de la zone de contenu.
  const scroller = tbodyEl.value.closest(".content");
  if (scroller) {
    const box = scroller.getBoundingClientRect();
    if (e.clientY < box.top + 48) scroller.scrollBy(0, -16);
    else if (e.clientY > box.bottom - 48) scroller.scrollBy(0, 16);
  }
}

function stopDrag() {
  drag.value = null;
  document.body.classList.remove("dragging-lines");
  window.removeEventListener("pointermove", onDragMove);
  window.removeEventListener("pointerup", endDrag);
  window.removeEventListener("pointercancel", stopDrag);
}

function endDrag() {
  const d = drag.value;
  stopDrag();
  if (!d?.moved) return;
  const block = lines.value.filter((l) => d.keys.has(l.key));
  const target = lines.value.find((l) => l.key === d.targetKey) ?? null;
  lines.value = moveBlockBefore(lines.value, block, target, entryLine());
}

onBeforeUnmount(stopDrag);

/** Trait d'insertion : sur la ligne devant laquelle on lâcherait (la ligne de saisie pour la fin). */
const dropKey = computed(() => {
  const d = drag.value;
  if (!d?.moved) return null;
  const key = d.targetKey ?? entryLine()?.key ?? null;
  return key != null && !d.keys.has(key) ? key : null;
});

// ---------- Sélection, copier / coller ----------

const { clipboard, copy, holdsLines } = useLineClipboard();
const selectedKeys = ref(new Set<number>());
let lastClickedKey: number | null = null;

const selectableLines = computed(() => lines.value.filter((l) => !isEntryLine(l)));
const selectedLines = computed(() => lines.value.filter((l) => selectedKeys.value.has(l.key)));
const allSelected = computed(
  () => selectableLines.value.length > 0 && selectableLines.value.every((l) => selectedKeys.value.has(l.key)),
);
const entryLine = () => {
  const last = lines.value[lines.value.length - 1];
  return last && isBlankItem(last) ? last : null;
};

/** Clic sur une case : Maj+clic sélectionne toute la plage depuis le clic précédent. */
function onSelectClick(line: EditLine, e: MouseEvent) {
  const keys = new Set(selectedKeys.value);
  const check = !keys.has(line.key);
  const from = lastClickedKey == null ? -1 : selectableLines.value.findIndex((l) => l.key === lastClickedKey);
  if (e.shiftKey && from >= 0) {
    const to = selectableLines.value.indexOf(line);
    const [a, b] = from < to ? [from, to] : [to, from];
    selectableLines.value.slice(a, b + 1).forEach((l) => (check ? keys.add(l.key) : keys.delete(l.key)));
  } else if (check) {
    keys.add(line.key);
  } else {
    keys.delete(line.key);
  }
  lastClickedKey = line.key;
  selectedKeys.value = keys;
}

function toggleAll() {
  selectedKeys.value = allSelected.value ? new Set() : new Set(selectableLines.value.map((l) => l.key));
}

const clearSelection = () => (selectedKeys.value = new Set());

/** Lignes concernées par copier / couper / supprimer / glisser (jamais la ligne de saisie vide). */
const selectionBlock = () => selectedLines.value.filter((l) => l !== entryLine());

/** Copie la sélection ; le texte part aussi dans le presse-papiers système (Excel, mail…). */
function copySelection(e?: ClipboardEvent) {
  const block = selectionBlock();
  if (!block.length) return;
  const text = copy(block, pricingKey.value);
  if (e?.clipboardData) e.clipboardData.setData("text/plain", text);
  else navigator.clipboard?.writeText(text).catch(() => {});
  toast.add({ severity: "info", summary: `${block.length} ligne(s) copiée(s)`, life: 1500 });
}

function deleteSelection() {
  const block = new Set(selectionBlock());
  lines.value = lines.value.filter((l) => !block.has(l));
  if (!entryLine()) lines.value.push(blankLine());
  clearSelection();
}

function cutSelection(e?: ClipboardEvent) {
  copySelection(e);
  deleteSelection();
}

/** Colle après `anchor` ; à défaut après la dernière ligne sélectionnée, sinon en fin de devis. */
async function pasteLines(anchor: EditLine | null = null) {
  if (!clipboard.value) return;
  const pasted: EditLine[] = clipboard.value.lines.map((l) => ({
    ...blankLine(),
    ...withTexts(l),
    resolvedRef: l.kind === "item" ? l.product_ref : null,
  }));
  const selection = selectedLines.value;
  anchor ??= selection.length ? selection[selection.length - 1] : null;
  lines.value = insertBlock(lines.value, pasted, anchor, entryLine());
  const pastedKeys = new Set(pasted.map((l) => l.key));
  selectedKeys.value = pastedKeys;

  // Copiées depuis un devis aux conditions différentes : on applique celles de ce devis.
  if (hasClient.value && clipboard.value.pricingKey !== pricingKey.value) {
    // On passe par lines.value (objets réactifs) pour que l'écran se mette à jour.
    const items = lines.value.filter((l) => pastedKeys.has(l.key) && l.kind === "item");
    await Promise.all(items.map((l) => resolve(l)));
    if (items.length) toast.add({ severity: "info", summary: "Prix recalculés pour ce client", life: 2500 });
  }
}

// ---------- Raccourcis clavier ⌘C / ⌘X / ⌘V / Échap (Ctrl sous Windows) ----------
// Le copier-coller de texte dans les champs reste prioritaire :
// - ⌘C / ⌘X copient les lignes sélectionnées, sauf si du texte est sélectionné dans le champ actif ;
// - ⌘V colle les lignes si le presse-papiers contient encore nos lignes (sinon, collage de texte normal),
//   juste après la ligne où se trouve le curseur.

function isTextField(el: EventTarget | null): el is HTMLElement {
  if (!(el instanceof HTMLElement)) return false;
  if (el.isContentEditable || el instanceof HTMLTextAreaElement) return true;
  return el instanceof HTMLInputElement && !["checkbox", "radio", "button"].includes(el.type);
}

function hasTextSelection(el: EventTarget | null) {
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    try {
      return el.selectionStart !== el.selectionEnd;
    } catch {
      return false;
    }
  }
  return !!window.getSelection()?.toString();
}

const copiesLines = (target: EventTarget | null) => selectedKeys.value.size > 0 && !hasTextSelection(target);

/** Ligne du tableau contenant l'élément (pour coller « ici »). */
function lineAt(el: EventTarget | null): EditLine | null {
  const row = el instanceof Element ? el.closest<HTMLElement>("tr[data-key]") : null;
  const key = row ? Number(row.dataset.key) : NaN;
  return lines.value.find((l) => l.key === key) ?? null;
}

function onCopy(e: ClipboardEvent) {
  if (!copiesLines(e.target)) return;
  e.preventDefault();
  copySelection(e);
}

function onCut(e: ClipboardEvent) {
  if (!copiesLines(e.target)) return;
  e.preventDefault();
  cutSelection(e);
}

function onPaste(e: ClipboardEvent) {
  if (!clipboard.value) return;
  const target = document.activeElement;
  // Dans un champ, on ne colle des lignes que si le presse-papiers contient toujours nos lignes.
  if (isTextField(target) && !holdsLines(e.clipboardData?.getData("text/plain") ?? "")) return;
  e.preventDefault();
  pasteLines(lineAt(target));
}

/** Lignes du devis en plein écran (toute la zone de contenu, menu visible), pour les gros devis. */
const fullscreen = ref(false);

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape" && selectedKeys.value.size && !isTextField(e.target)) {
    clearSelection();
    return;
  }
  // Échap quitte le plein écran (sauf si elle a déjà servi, par ex. à fermer une liste de suggestions).
  if (e.key === "Escape" && fullscreen.value && !e.defaultPrevented) {
    fullscreen.value = false;
    return;
  }
  if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
  const key = e.key.toLowerCase();
  // Hors champ texte, WebKit n'émet pas toujours copy / cut / paste : on agit dès la touche.
  if ((key === "c" || key === "x") && copiesLines(e.target) && !isTextField(e.target)) {
    e.preventDefault();
    if (key === "c") copySelection();
    else cutSelection();
  } else if ((key === "c" || key === "x") && copiesLines(e.target)) {
    // Dans un champ sans texte sélectionné, « Copier » est inactif : on déclenche l'événement nous-mêmes.
    e.preventDefault();
    if (!document.execCommand(key === "c" ? "copy" : "cut")) {
      if (key === "c") copySelection();
      else cutSelection();
    }
  } else if (key === "v" && clipboard.value && !isTextField(e.target)) {
    e.preventDefault();
    pasteLines(lineAt(e.target));
  }
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  document.addEventListener("copy", onCopy);
  document.addEventListener("cut", onCut);
  document.addEventListener("paste", onPaste);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  document.removeEventListener("copy", onCopy);
  document.removeEventListener("cut", onCut);
  document.removeEventListener("paste", onPaste);
});

function removeLine(line: EditLine) {
  lines.value = lines.value.filter((l) => l !== line);
  if (!lines.value.length) lines.value.push(blankLine());
}

/** Prix net de la ligne (remise supplémentaire comprise) sous le prix seuil. */
const isBelowThreshold = (l: EditLine) =>
  l.threshold_price != null && netUnitPrice(l.unit_price, l.discount) < l.threshold_price;

// ---------- Nettoyage des lignes ----------

/** Nettoyage proposé (ligne de saisie exclue) : articles sans quantité, lignes vides, orphelins. */
const cleanup = computed(() => cleanLines(lines.value.filter((l) => !isEntryLine(l))));
const cleanupCount = computed(
  () => cleanup.value.noQuantity.length + cleanup.value.empty.length + cleanup.value.orphans.length,
);

function cleanUpLines() {
  const r = cleanup.value;
  if (!cleanupCount.value) return;
  const parts = [
    r.noQuantity.length && `${r.noQuantity.length} article(s) sans quantité`,
    r.empty.length && `${r.empty.length} ligne(s) vide(s)`,
    r.orphans.length && `${r.orphans.length} titre(s), texte(s) ou sous-total(s) qui n'accompagnent plus aucun article`,
  ].filter(Boolean);
  confirm.require({
    header: "Nettoyer les lignes",
    message: `Supprimer ${parts.join(", ")} ?`,
    icon: "pi pi-eraser",
    rejectProps: { label: "Annuler", severity: "secondary", text: true },
    acceptProps: { label: `Supprimer ${cleanupCount.value} ligne(s)`, severity: "danger" },
    accept: () => {
      const n = cleanupCount.value;
      lines.value = [...cleanup.value.kept, blankLine()];
      clearSelection();
      toast.add({ severity: "success", summary: `${n} ligne(s) supprimée(s)`, life: 2000 });
    },
  });
}

/** Champs enregistrés d'une ligne (sans l'état d'édition). */
const toQuoteLine = (l: EditLine): QuoteLine => ({
  kind: l.kind,
  product_ref: l.product_ref,
  enedis_code: l.enedis_code ?? null,
  designation: l.designation,
  quantity: l.quantity,
  unit_price: l.unit_price,
  discount: l.discount ?? 0,
  is_option: !!l.is_option,
  price_source: l.price_source,
  public_price: l.public_price,
  discounted_price: l.discounted_price ?? null,
  lpn_price: l.lpn_price ?? null,
  lpn_list: l.lpn_list ?? null,
  threshold_price: l.threshold_price,
});

function toQuote(): Quote {
  return {
    id: quoteId.value,
    number: number.value,
    client_code: clientCode.value,
    client_name: clientName.value.trim(),
    ...contact.value,
    ...pricing.value,
    date: date.value,
    notes: notes.value.trim() || null,
    total_ht: total.value,
    discount_pct: globalDiscount.value ?? 0,
    total_net: netTotal.value,
    total_options: totals.value.options,
    lines: [
      ...lines.value
        // Articles sans réf et lignes de texte vides : ignorés.
        .filter((l) => (l.kind === "item" ? l.product_ref.trim() : l.kind === "subtotal" || l.designation.trim()))
        .map((l) => ({
          ...toQuoteLine(l),
          designation: l.kind === "subtotal" ? l.designation.trim() || "Sous-total" : l.designation,
          quantity: l.quantity ?? 0,
          discount: l.discount ?? 0,
        })),
      // Frais toujours en dernier.
      ...feeLines.value.map((l) => ({
        ...toQuoteLine(l),
        designation: l.designation.trim() || FEE_LABELS[l.kind as FeeKind],
        quantity: 1,
      })),
    ],
  };
}

async function save(): Promise<Quote | null> {
  if (!hasClient.value) {
    toast.add({ severity: "warn", summary: "Choisissez un client", life: 2500 });
    return null;
  }
  if (!filledLines.value.length) {
    toast.add({ severity: "warn", summary: "Ajoutez au moins une ligne", life: 2500 });
    return null;
  }
  saving.value = true;
  try {
    const saved = await api.saveQuote(toQuote());
    const isNew = quoteId.value == null;
    quoteId.value = saved.id;
    number.value = saved.number;
    // Enregistré en base : plus besoin du brouillon.
    if (isNew) removeDraft("nouveau");
    markSaved();
    if (isNew) router.replace(`/devis/${saved.id}`);
    toast.add({ severity: "success", summary: `Devis ${saved.number} enregistré`, life: 2000 });
    return saved;
  } catch (e) {
    toast.add({ severity: "error", summary: "Enregistrement", detail: errorMessage(e) });
    return null;
  } finally {
    saving.value = false;
  }
}

/** Suivi « Affaire obtenue » (devis devenu commande / facture) : date du marquage. */
const wonAt = ref<string | null>(null);
async function setWon(won: boolean) {
  if (quoteId.value == null) return;
  try {
    wonAt.value = await api.setQuoteWon(quoteId.value, won);
    // Une seule version obtenue : les autres sont décochées.
    for (const v of versions.value) v.won_at = v.id === quoteId.value ? wonAt.value : won ? null : v.won_at;
  } catch (e) {
    toast.add({ severity: "error", summary: "Affaire obtenue", detail: errorMessage(e) });
  }
}

/** Nouvelle version du devis (enregistré d'abord s'il a des modifications), puis ouverture. */
const creatingVersion = ref(false);
async function newVersion() {
  if (quoteId.value == null) return;
  if (hasDraft.value && !(await save())) return;
  creatingVersion.value = true;
  try {
    const v = await api.newQuoteVersion(quoteId.value, todayIso());
    toast.add({ severity: "success", summary: `Version ${v.number} créée`, life: 2500 });
    router.push(`/devis/${v.id}`);
  } catch (e) {
    toast.add({ severity: "error", summary: "Nouvelle version", detail: errorMessage(e) });
  } finally {
    creatingVersion.value = false;
  }
}

// PDF + documentation technique : le devis est enregistré, puis la fenêtre de choix des documents s'ouvre.
const docsDialogVisible = ref(false);
const docsDialogQuote = ref<Quote | null>(null);

async function savePdfWithDocs() {
  const saved = await save();
  if (!saved) return;
  docsDialogQuote.value = saved;
  docsDialogVisible.value = true;
}

async function savePdf() {
  const saved = await save();
  if (!saved) return;
  try {
    const result = await exportQuotePdf(saved);
    if (result) {
      toast.add({ severity: "success", summary: "PDF enregistré", detail: result.path, life: 4000 });
      if (result.warning) toast.add({ severity: "warn", summary: "Conditions générales", detail: result.warning });
    }
  } catch (e) {
    toast.add({ severity: "error", summary: "Génération du PDF", detail: errorMessage(e) });
  }
}

/** Devis enregistrés avant l'ajout des colonnes prix public / seuil : on les complète (sans toucher au PU). */
async function fillMissingReferencePrices() {
  if (!hasClient.value) return;
  const ctx = pricing.value;
  const missing = lines.value.filter(
    (l) =>
      l.kind === "item" &&
      l.product_ref &&
      (l.public_price == null || (l.discounted_price == null && l.lpn_price == null)),
  );
  await Promise.all(
    missing.map(async (l) => {
      try {
        const p = await api.resolvePrice(ctx, l.product_ref);
        Object.assign(l, {
          public_price: p.public_price,
          threshold_price: p.threshold_price,
          discounted_price: p.discounted_price,
          lpn_price: p.lpn_price,
          lpn_list: p.lpn_list,
          enedis_code: l.enedis_code ?? p.enedis_code,
        });
      } catch {
        // Produit disparu du catalogue : colonnes laissées vides.
      }
    }),
  );
}

// ---------- Brouillon local : le devis en cours est gardé à chaque modification ----------

const draftKey = () => (quoteId.value == null ? "nouveau" : String(quoteId.value));
/** Faux pendant un chargement : on n'écrit pas de brouillon d'un état à moitié chargé. */
const draftReady = ref(false);
/** État tel qu'en base (ou vierge) : tant qu'on y est, pas de brouillon. */
let savedSnapshot = "";
const hasDraft = ref(false);

/** Ce qui compte pour savoir si le devis a changé (pas le prix public / seuil complétés en tâche de fond). */
const snapshot = () =>
  JSON.stringify({
    client: [clientCode.value, clientName.value, pricingKey.value, contact.value],
    date: date.value,
    notes: notes.value,
    discount: globalDiscount.value,
    lines: [...lines.value.filter((l) => !isEntryLine(l)), ...feeLines.value].map((l) => [
      l.kind,
      l.product_ref,
      l.designation,
      l.quantity,
      l.unit_price,
      l.discount,
      l.is_option,
      l.price_source,
    ]),
  });

function markSaved() {
  savedSnapshot = snapshot();
  removeDraft(draftKey());
  hasDraft.value = false;
}

function persistDraft() {
  if (!draftReady.value) return;
  if (snapshot() === savedSnapshot) {
    removeDraft(draftKey());
    hasDraft.value = false;
    return;
  }
  const draft: QuoteDraft = {
    number: number.value,
    client_code: clientCode.value,
    client_name: clientName.value,
    ...contact.value,
    ...pricing.value,
    date: date.value,
    notes: notes.value,
    discount_pct: globalDiscount.value,
    lines: [...lines.value.filter((l) => !isEntryLine(l)), ...feeLines.value].map(toQuoteLine),
    savedAt: new Date().toISOString(),
  };
  writeDraft(draftKey(), draft);
  hasDraft.value = true;
}

let draftTimer: ReturnType<typeof setTimeout> | undefined;
watch([clientCode, clientName, pricingKey, contact, date, notes, globalDiscount, lines], () => {
  clearTimeout(draftTimer);
  draftTimer = setTimeout(persistDraft, 300);
}, { deep: true });
onBeforeUnmount(() => {
  clearTimeout(draftTimer);
  persistDraft();
});
window.addEventListener("beforeunload", persistDraft);
onBeforeUnmount(() => window.removeEventListener("beforeunload", persistDraft));

function applyDraft(draft: QuoteDraft) {
  date.value = draft.date;
  notes.value = draft.notes;
  globalDiscount.value = draft.discount_pct ?? 0;
  const known = draft.client_code ? clients.value.find((c) => c.code === draft.client_code) : undefined;
  setClientFields({
    client_code: draft.client_code,
    client_name: draft.client_name,
    // Brouillon d'avant les conditions propres au devis : on reprend celles du client.
    discount_cfa: draft.discount_cfa ?? known?.discount_cfa ?? 0,
    discount_cfo: draft.discount_cfo ?? known?.discount_cfo ?? 0,
    price_lists: draft.price_lists ?? known?.price_lists ?? [],
    forced_price_list: draft.forced_price_list ?? null,
    contact_name: draft.contact_name,
    contact_email: draft.contact_email,
    contact_phone: draft.contact_phone,
    sales_rep: draft.sales_rep ?? "",
  });
  const restored = draft.lines.map((l) => ({
    ...blankLine(),
    ...withTexts(l),
    resolvedRef: l.kind === "item" ? l.product_ref : null,
  }));
  lines.value = [...restored.filter((l) => !isFee(l.kind)), blankLine()];
  feeLines.value = restored.filter((l) => isFee(l.kind));
  hasDraft.value = true;
}

/** Vide le devis en cours (et son brouillon) ; pour un devis enregistré, ouvre un nouveau devis vierge. */
function startOver() {
  confirm.require({
    header: "Repartir de zéro",
    message: quoteId.value
      ? "Abandonner les modifications non enregistrées et commencer un nouveau devis vierge ? Le devis enregistré n'est pas modifié."
      : "Vider le devis en cours (client, lignes, notes) ?",
    icon: "pi pi-refresh",
    rejectProps: { label: "Annuler", severity: "secondary", text: true },
    acceptProps: { label: "Repartir de zéro", severity: "danger" },
    accept: () => {
      clearTimeout(draftTimer);
      draftReady.value = false;
      removeDraft(draftKey());
      removeDraft("nouveau");
      if (quoteId.value) router.push("/devis/nouveau");
      else load(undefined);
    },
  });
}

/** Client et conditions du devis ; la recherche affiche le client de la base s'il existe encore. */
function setClientFields(q: Pick<Quote, "client_code" | "client_name"> & PricingContext & Partial<QuoteContact>) {
  contact.value = {
    contact_name: q.contact_name ?? "",
    contact_email: q.contact_email ?? "",
    contact_phone: q.contact_phone ?? "",
    sales_rep: q.sales_rep ?? "",
  };
  clientCode.value = q.client_code;
  clientName.value = q.client_name;
  discountCfa.value = q.discount_cfa;
  discountCfo.value = q.discount_cfo;
  priceLists.value = [...q.price_lists];
  forcedList.value = q.forced_price_list ?? null;
  client.value = q.client_code
    ? (clients.value.find((c) => c.code === q.client_code) ??
      // Client disparu depuis un réimport : on affiche ce que le devis a mémorisé.
      ({ code: q.client_code, name: q.client_name, price_lists: [] } as unknown as Client))
    : null;
}

async function load(id: string | undefined) {
  dismissedFees.clear();
  clearSelection();
  clearTimeout(draftTimer);
  draftReady.value = false;
  try {
    await loadSaved(id);
    savedSnapshot = snapshot();
    hasDraft.value = false;
    // Modifications non enregistrées laissées la dernière fois : on les reprend.
    let draft = readDraft(draftKey());
    // Brouillon d'un devis supprimé dont l'identifiant a été réutilisé : on l'écarte.
    if (draft && draft.number !== undefined && (draft.number ?? null) !== number.value) {
      removeDraft(draftKey());
      draft = null;
    }
    if (draft) {
      applyDraft(draft);
      toast.add({
        severity: "info",
        summary: "Brouillon restauré",
        detail: "Les modifications non enregistrées ont été reprises.",
        life: 3000,
      });
    }
  } finally {
    await nextTick();
    draftReady.value = true;
  }
}

async function loadSaved(id: string | undefined) {
  try {
    if (!id) {
      // Nouveau devis : formulaire vierge.
      quoteId.value = null;
      number.value = null;
      versions.value = [];
      wonAt.value = null;
      date.value = todayIso();
      notes.value = "";
      globalDiscount.value = 0;
      setClientFields({
        client_code: null,
        client_name: "",
        discount_cfa: 0,
        discount_cfo: 0,
        price_lists: [],
        forced_price_list: null,
        sales_rep: lastSalesRep(),
      });
      lines.value = [blankLine()];
      feeLines.value = [];
      return;
    }
    const q = await api.getQuote(Number(id));
    quoteId.value = q.id;
    number.value = q.number;
    versions.value = q.versions ?? [];
    wonAt.value = q.won_at ?? null;
    date.value = q.date;
    notes.value = q.notes ?? "";
    globalDiscount.value = q.discount_pct;
    setClientFields(q);
    const loaded: EditLine[] = q.lines.map((l) => ({
      ...withTexts({
        ...l,
        // Une quantité enregistrée à 0 revient vide, comme à la saisie.
        quantity: l.kind === "item" && !l.quantity ? null : l.quantity,
        unit_price: round2(l.unit_price),
      }),
      key: nextKey++,
      resolvedRef: l.product_ref,
      pendingRef: null,
      error: null,
    }));
    lines.value = [...loaded.filter((l) => !isFee(l.kind)), blankLine()];
    feeLines.value = loaded.filter((l) => isFee(l.kind));
    fillMissingReferencePrices();
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement du devis", detail: errorMessage(e) });
  }
}

// Le composant est réutilisé entre /devis/nouveau et /devis/:id (bouton « Nouveau devis », duplication).
// Après la création d'un devis, l'URL passe à /devis/:id sans recharger : quoteId est déjà à jour.
watch(
  () => props.id,
  (id) => {
    // Autre devis (ou nouveau) : on sort du plein écran.
    fullscreen.value = false;
    if (id !== (quoteId.value == null ? undefined : String(quoteId.value))) load(id);
  },
);

onMounted(async () => {
  try {
    const [list, priceListRows, settings] = await Promise.all([
      api.listClients(),
      api.listPriceLists(),
      api.getSettings(),
    ]);
    clients.value = list;
    allPriceLists.value = priceListRows;
    favorites.value = favoriteLists(settings);
    salesReps.value = salesRepList(settings);
    feeRules.value = feeRulesFrom(settings);
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des clients", detail: errorMessage(e) });
  }
  await load(props.id);
});
</script>

<template>
  <div class="page">
    <div class="page-header">
      <Button icon="pi pi-arrow-left" text rounded @click="router.push('/devis')" />
      <h1>{{ number ? `Devis ${number}` : "Nouveau devis" }}</h1>
      <!-- Versions du devis : liens de l'une à l'autre, la version obtenue cochée. -->
      <nav v-if="versions.length > 1" class="versions" aria-label="Versions du devis">
        <RouterLink
          v-for="v in versions"
          :key="v.id"
          v-tooltip.bottom="`${v.number} du ${formatDate(v.date)}${v.won_at ? ' · affaire obtenue' : ''}`"
          :to="`/devis/${v.id}`"
          class="version"
          :class="{ current: v.id === quoteId }"
        >
          V{{ v.version }}<i v-if="v.won_at" class="pi pi-check" />
        </RouterLink>
      </nav>
      <label
        v-if="quoteId != null"
        v-tooltip.bottom="wonAt ? `Cochée le ${formatDate(wonAt)}` : 'Le devis est devenu une commande / facture'"
        class="won-toggle"
        :class="{ on: wonAt }"
      >
        <Checkbox :model-value="!!wonAt" binary @update:model-value="setWon" />
        Affaire obtenue
      </label>
      <span
        v-if="hasDraft"
        v-tooltip.bottom="'Gardé sur cet ordinateur ; restauré si vous revenez sur ce devis'"
        class="muted draft-badge"
      >
        <i class="pi pi-circle-fill" /> Modifications non enregistrées
      </span>
      <span class="spacer" />
      <Button
        v-if="quoteId != null"
        v-tooltip.bottom="'Copie de ce devis sous le même numéro, suffixé -V2, -V3…'"
        label="Nouvelle version"
        icon="pi pi-history"
        severity="secondary"
        text
        :loading="creatingVersion"
        @click="newVersion"
      />
      <Button label="Repartir de zéro" icon="pi pi-refresh" severity="secondary" text @click="startOver" />
      <Button label="Enregistrer" icon="pi pi-save" severity="secondary" :loading="saving" @click="save" />
      <Button label="Générer PDF" icon="pi pi-file-pdf" :loading="saving" @click="savePdf" />
      <Button
        v-tooltip.bottom="'Devis suivi de la documentation technique des produits'"
        label="Devis PDF + Docs"
        icon="pi pi-book"
        :loading="saving"
        @click="savePdfWithDocs"
      />
    </div>
    <QuoteDocsDialog v-model:visible="docsDialogVisible" :quote="docsDialogQuote" />
    <PriceListDrawer v-model:code="drawerList" />

    <div class="card head">
      <div class="field client-field">
        <div class="client-picker">
          <AutoComplete
            v-model="client"
            input-id="client"
            :suggestions="clientSuggestions"
            option-label="name"
            placeholder="Rechercher un client (nom ou code)…"
            force-selection
            fluid
            @complete="searchClients"
            @option-select="(e: { value: Client }) => applyClient(e.value)"
            @keydown.tab="onClientTab"
          >
            <template #option="{ option }">
              <div class="opt">
                <span>{{ option.name }}</span>
                <span class="muted mono">{{ option.code }}</span>
              </div>
            </template>
            <template #empty>
              <span class="muted">Aucun client trouvé — « Client ponctuel » pour un client hors base</span>
            </template>
          </AutoComplete>
          <Button
            v-tooltip.bottom="'Client qui n\'existe que dans ce devis'"
            label="Client ponctuel"
            icon="pi pi-user-plus"
            severity="secondary"
            outlined
            @click="openEphemeral()"
          />
        </div>

        <Dialog v-model:visible="ephemeralVisible" modal header="Client ponctuel" :style="{ width: '480px' }">
          <p class="muted dialog-hint">Ce client n'est pas ajouté à la base : il n'existe que dans ce devis.</p>
          <form class="form-grid" @submit.prevent="applyEphemeral">
            <label for="eph-name">Raison sociale *</label>
            <InputText id="eph-name" v-model="ephemeral.name" autofocus />
            <label for="eph-cfa">Remise CFA</label>
            <InputNumber v-model="ephemeral.cfa" input-id="eph-cfa" :min="0" :max="100" :max-fraction-digits="2" suffix=" %" />
            <label for="eph-cfo">Remise CFO</label>
            <InputNumber v-model="ephemeral.cfo" input-id="eph-cfo" :min="0" :max="100" :max-fraction-digits="2" suffix=" %" />
            <label>Liste de prix</label>
            <Select
              v-model="ephemeral.list"
              :options="priceListOptions"
              option-label="label"
              option-value="code"
              filter
              show-clear
              placeholder="Aucune (prix public − remises)"
              :virtual-scroller-options="{ itemSize: 36 }"
            />
            <button type="submit" hidden />
          </form>
          <template #footer>
            <Button label="Annuler" severity="secondary" text @click="ephemeralVisible = false" />
            <Button label="Valider" icon="pi pi-check" @click="applyEphemeral" />
          </template>
        </Dialog>
      </div>
      <div class="field">
        <InputText id="date" v-model="date" v-tooltip.bottom="'Date du devis'" aria-label="Date du devis" type="date" />
      </div>

      <!-- Client du devis : raison sociale, conditions et contact, modifiables ici sans toucher à la fiche client. -->
      <div v-if="hasClient" class="client-details">
        <span class="cd-caption">Client</span>
        <div class="cd-fields">
          <InputText
            v-model="clientName"
            v-tooltip.bottom="'Raison sociale imprimée sur le devis'"
            aria-label="Raison sociale"
            placeholder="Raison sociale"
            size="small"
            class="cd-name"
          />
          <div class="field">
            <div class="cd-inline">
              <Tag v-if="clientCode" :value="`Code ${clientCode}`" severity="secondary" />
              <Tag v-else value="Client ponctuel" severity="info" />
              <Button
                v-tooltip.bottom="'Retirer le client de ce devis (les prix sont recalculés sans remise)'"
                label="Retirer le client"
                icon="pi pi-user-minus"
                severity="secondary"
                text
                size="small"
                @click="clearClient"
              />
            </div>
          </div>
        </div>

        <span class="cd-caption">Conditions</span>
        <div class="cd-fields">
          <div class="field">
            <label for="cfa">Remise CFA</label>
            <InputNumber
              v-model="discountCfa"
              input-id="cfa"
              :min="0"
              :max="100"
              :max-fraction-digits="2"
              suffix=" %"
              size="small"
              input-class="pct"
              @update:model-value="repriceLines(true)"
            />
          </div>
          <div class="field">
            <label for="cfo">Remise CFO</label>
            <InputNumber
              v-model="discountCfo"
              input-id="cfo"
              :min="0"
              :max="100"
              :max-fraction-digits="2"
              suffix=" %"
              size="small"
              input-class="pct"
              @update:model-value="repriceLines(true)"
            />
          </div>
          <div class="field">
            <label>Listes de prix</label>
            <div v-if="clientCode" class="cd-inline">
              <button
                v-for="l in priceLists"
                :key="l"
                v-tooltip.bottom="'Voir les produits de cette liste'"
                type="button"
                class="list-link"
                @click="openPriceList(l)"
              >
                <Tag :value="l" severity="secondary" />
              </button>
              <span v-if="!priceLists.length" class="muted">aucune</span>
            </div>
            <Select
              v-else
              v-model="ephemeralList"
              :options="priceListOptions"
              option-label="label"
              option-value="code"
              filter
              show-clear
              placeholder="Aucune liste de prix"
              size="small"
              class="list-select"
              :virtual-scroller-options="{ itemSize: 36 }"
            />
            <Button
              v-if="!clientCode && ephemeralList"
              v-tooltip.bottom="'Voir les produits de cette liste'"
              icon="pi pi-list"
              text
              rounded
              size="small"
              severity="secondary"
              aria-label="Voir les produits de la liste"
              @click="openPriceList(ephemeralList)"
            />
          </div>
          <div class="field">
            <label v-tooltip.top="'Pour ce devis uniquement'" for="forced-list">Liste forcée</label>
            <Select
              v-model="forcedList"
              input-id="forced-list"
              :options="forcedOptions"
              option-label="label"
              option-value="code"
              show-clear
              :placeholder="forcedOptions.length ? 'Aucune' : 'Aucune favorite (Réglages)'"
              :disabled="!forcedOptions.length"
              size="small"
              class="list-select"
              @change="onForcedListChange"
            />
            <Button
              v-tooltip.bottom="'Voir les produits de la liste forcée'"
              icon="pi pi-list"
              text
              rounded
              size="small"
              severity="secondary"
              :disabled="!forcedList"
              aria-label="Voir les produits de la liste forcée"
              @click="forcedList && openPriceList(forcedList)"
            />
          </div>
        </div>

        <!-- Contact chez le client et commercial : imprimés sur le PDF. -->
        <span class="cd-caption">Contact</span>
        <div class="cd-fields">
          <div class="field">
            <label for="contact-name">Nom</label>
            <InputText id="contact-name" v-model="contact.contact_name" size="small" class="contact-name" />
          </div>
          <div class="field">
            <label for="contact-email">Email</label>
            <InputText id="contact-email" v-model="contact.contact_email" size="small" type="email" class="contact-email" />
          </div>
          <div class="field">
            <label for="contact-phone">Téléphone</label>
            <InputText id="contact-phone" v-model="contact.contact_phone" size="small" class="contact-phone" />
          </div>
          <div class="field">
            <label for="sales-rep">Commercial</label>
            <Select
              :model-value="contact.sales_rep || null"
              input-id="sales-rep"
              :options="salesRepOptions"
              show-clear
              :placeholder="salesRepOptions.length ? 'Choisir…' : 'Aucun (à ajouter dans Réglages)'"
              size="small"
              class="sales-rep"
              @update:model-value="(v: string | null) => rememberSalesRep((contact.sales_rep = v ?? ''))"
            />
          </div>
        </div>
      </div>
    </div>

    <div class="card" :class="{ 'lines-fullscreen': fullscreen }">
      <!-- Lignes sélectionnées : copier / couper / supprimer. Le collage se fait au clavier (Ctrl+V / ⌘V),
           après la dernière ligne sélectionnée ou en fin de devis. -->
      <div v-if="selectedKeys.size" class="selection-bar">
        <strong>{{ selectedKeys.size }} sélectionnée(s)</strong>
        <Button v-tooltip.bottom="`${MOD}C, puis ${MOD}V pour coller`" label="Copier" icon="pi pi-copy" text size="small" @click="copySelection()" />
        <Button v-tooltip.bottom="`${MOD}X, puis ${MOD}V pour coller`" label="Couper" icon="pi pi-clone" text size="small" @click="cutSelection()" />
        <Button label="Supprimer" icon="pi pi-trash" text size="small" severity="danger" @click="deleteSelection" />
        <Button v-tooltip.bottom="'Échap'" icon="pi pi-times" text rounded size="small" severity="secondary" @click="clearSelection" />
      </div>
      <table ref="linesTable" class="lines">
        <thead>
          <tr>
            <th style="width: 26px" class="fullscreen-cell">
              <button
                v-tooltip.right="fullscreen ? 'Quitter le plein écran (Échap)' : 'Plein écran'"
                type="button"
                class="fullscreen-btn"
                :aria-label="fullscreen ? 'Quitter le plein écran' : 'Lignes en plein écran'"
                :aria-pressed="fullscreen"
                @click="fullscreen = !fullscreen"
              >
                <i :class="fullscreen ? 'pi pi-window-minimize' : 'pi pi-window-maximize'" />
              </button>
            </th>
            <th style="width: 26px" class="select-cell">
              <input
                type="checkbox"
                :checked="allSelected"
                :disabled="!selectableLines.length"
                aria-label="Tout sélectionner"
                @click="toggleAll"
              />
            </th>
            <th style="width: 78px">ENEDIS</th>
            <th style="width: 100px">Référence</th>
            <th>Désignation</th>
            <th style="width: 56px" class="num">Qté</th>
            <th style="width: 76px" class="num">Prix public</th>
            <th v-tooltip.top="'Prix public − remise CFA / CFO du devis'" style="width: 76px" class="num">Prix remisé</th>
            <th v-tooltip.top="'Prix négocié de la liste de prix'" style="width: 76px" class="num">LPN</th>
            <th style="width: 80px" class="num">PU HT</th>
            <th v-tooltip.top="'Remise supplémentaire sur le prix de la ligne'" style="width: 64px" class="num">
              Remise sup. (%)
            </th>
            <th ref="totalHeader" style="width: 100px" class="num">Total HT</th>
            <th v-tooltip.top="'Ligne en option : hors total HT, comptée dans « Total options »'" style="width: 48px" class="select-cell">
              Option
            </th>
            <th v-tooltip.top="'Non imprimé sur le devis'" style="width: 70px" class="num seuil">Prix seuil</th>
            <th style="width: 30px"></th>
          </tr>
        </thead>
        <tbody ref="tbodyEl">
          <template v-for="line in lines" :key="line.key">
            <tr
              v-if="line.kind !== 'item'"
              :data-key="line.key"
              :class="[
                `${line.kind}-row`,
                {
                  selected: selectedKeys.has(line.key),
                  dragging: drag?.moved && drag.keys.has(line.key),
                  'drop-before': dropKey === line.key,
                },
              ]"
            >
              <td
                v-tooltip.left="'Glisser pour déplacer'"
                class="drag-cell drag-handle"
                @pointerdown="startDrag(line, $event)"
              >
                <i class="pi pi-bars" />
              </td>
              <td class="select-cell">
                <input
                  type="checkbox"
                  :checked="selectedKeys.has(line.key)"
                  aria-label="Sélectionner la ligne"
                  @click="onSelectClick(line, $event)"
                />
              </td>
              <template v-if="line.kind === 'text'">
                <td colspan="11">
                  <Textarea
                    :id="`text-${line.key}`"
                    v-model="line.designation"
                    rows="1"
                    auto-resize
                    fluid
                    size="small"
                    placeholder="Texte libre…"
                    class="text-line"
                  />
                </td>
                <td></td>
              </template>
              <!-- Titre : sépare le devis en paragraphes (gras, rouge ; plus grand sur le PDF). -->
              <template v-else-if="line.kind === 'title'">
                <td colspan="11">
                  <InputText
                    :id="`text-${line.key}`"
                    v-model="line.designation"
                    fluid
                    size="small"
                    placeholder="Titre…"
                    class="title-line"
                  />
                </td>
                <td></td>
              </template>
              <template v-else>
                <td colspan="9">
                  <InputText v-model="line.designation" fluid size="small" class="subtotal-label" />
                </td>
                <td class="num subtotal-amount">{{ formatEuro(subtotalAmounts.get(line.key)) }}</td>
                <td></td>
                <td></td>
              </template>
              <td>
                <Button
                  icon="pi pi-times"
                  text
                  rounded
                  size="small"
                  severity="secondary"
                  @click="removeLine(line)"
                />
              </td>
            </tr>
            <tr
              v-else
              :data-key="line.key"
              :class="{
                selected: selectedKeys.has(line.key),
                'option-row': line.is_option,
                dragging: drag?.moved && drag.keys.has(line.key),
                'drop-before': dropKey === line.key,
              }"
            >
              <td
                v-if="!isEntryLine(line)"
                v-tooltip.left="'Glisser pour déplacer'"
                class="drag-cell drag-handle"
                @pointerdown="startDrag(line, $event)"
              >
                <i class="pi pi-bars" />
              </td>
              <td v-else class="drag-cell"></td>
              <td class="select-cell">
                <input
                  v-if="!isEntryLine(line)"
                  type="checkbox"
                  :checked="selectedKeys.has(line.key)"
                  aria-label="Sélectionner la ligne"
                  @click="onSelectClick(line, $event)"
                />
              </td>
              <td>
                <AutoComplete
                  :model-value="line.enedis_code ?? ''"
                  :input-id="`enedis-${line.key}`"
                  :suggestions="enedisSuggestions"
                  option-label="enedis_code"
                  :delay="200"
                  :min-length="2"
                  :show-empty-message="false"
                  fluid
                  size="small"
                  placeholder="Code…"
                  input-class="mono enedis-input"
                  @update:model-value="(v: string | ProductHit) => (line.enedis_code = typeof v === 'string' ? v : (v.enedis_code ?? ''))"
                  @complete="searchEnedis($event, line)"
                  @option-select="commitEnedis(line, true)"
                  @keydown.enter="commitEnedis(line, true)"
                  @keydown.tab="validateAndGoToQuantity(line, 'enedis', $event)"
                  @blur="commitEnedis(line, false)"
                >
                  <template #option="{ option }">
                    <div class="opt">
                      <span class="mono">{{ option.enedis_code }}</span>
                      <span class="mono muted">{{ option.ref }}</span>
                      <span class="muted">{{ option.designation }}</span>
                    </div>
                  </template>
                </AutoComplete>
              </td>
              <td>
                <AutoComplete
                  :model-value="line.product_ref"
                  :input-id="`ref-${line.key}`"
                  :suggestions="productSuggestions"
                  option-label="ref"
                  :delay="200"
                  :min-length="2"
                  :show-empty-message="false"
                  fluid
                  size="small"
                  placeholder="Réf…"
                  :invalid="!!line.error"
                  @update:model-value="(v: string | ProductHit) => (line.product_ref = typeof v === 'string' ? v : v.ref)"
                  @complete="searchProducts($event, line)"
                  @option-select="commitRef(line)"
                  @blur="onRefBlur(line)"
                  @keydown.enter="commitRef(line)"
                  @keydown.tab="validateAndGoToQuantity(line, 'ref', $event)"
                >
                  <template #option="{ option }">
                    <div class="opt">
                      <span class="mono">{{ option.ref }}</span>
                      <span v-if="option.enedis_code" class="mono muted">{{ option.enedis_code }}</span>
                      <span class="muted">{{ option.designation }}</span>
                    </div>
                  </template>
                </AutoComplete>
              </td>
              <td>
                <InputText v-model="line.designation" fluid size="small" />
                <small v-if="line.error" class="warn">{{ line.error }}</small>
              </td>
              <td>
                <InputText
                  :id="`qty-${line.key}`"
                  :model-value="line.qtyText"
                  inputmode="decimal"
                  autocomplete="off"
                  fluid
                  size="small"
                  class="num"
                  @update:model-value="(v: string | undefined) => onQtyInput(line, v ?? '')"
                  @blur="line.qtyText = editText(line.quantity, 3)"
                  @keydown.enter="moveToQuantity(line, 1, $event)"
                  @keydown.down="moveToQuantity(line, 1, $event)"
                  @keydown.up="moveToQuantity(line, -1, $event)"
                />
              </td>
              <td class="num muted">{{ formatUnitPrice(line.public_price) }}</td>
              <!-- En gras : le prix qui a fixé le PU HT (le plus bas des deux, ou la liste forcée). -->
              <td class="num ref-price" :class="{ used: line.price_source?.startsWith('Public') }">
                {{ formatUnitPrice(line.discounted_price) }}
              </td>
              <td
                v-tooltip.top="line.lpn_list ? `Liste ${line.lpn_list}` : 'Aucun prix négocié'"
                class="num ref-price"
                :class="{ used: line.lpn_price != null && line.price_source === line.lpn_list }"
              >
                {{ formatUnitPrice(line.lpn_price) }}
              </td>
              <!-- Source du prix en infobulle (sur une 2e ligne, elle doublait la hauteur des lignes). -->
              <td
                v-tooltip.top="
                  line.price_source
                    ? `Prix : ${line.price_source}${isBelowThreshold(line) ? ' · sous le prix seuil' : ''}`
                    : ''
                "
              >
                <div class="euro-input">
                  <InputText
                    :model-value="line.priceText"
                    inputmode="decimal"
                    autocomplete="off"
                    fluid
                    size="small"
                    class="num"
                    :invalid="isBelowThreshold(line)"
                    @update:model-value="(v: string | undefined) => onPriceInput(line, v ?? '')"
                    @blur="onPriceBlur(line)"
                  />
                  <span v-if="line.product_ref">€</span>
                </div>
              </td>
              <td>
                <InputText
                  :model-value="line.discountText"
                  inputmode="decimal"
                  autocomplete="off"
                  placeholder="–"
                  fluid
                  size="small"
                  class="num"
                  @update:model-value="(v: string | undefined) => onDiscountInput(line, v ?? '')"
                  @blur="line.discountText = line.discount ? editText(line.discount, 3) : ''"
                />
              </td>
              <td class="num">{{ line.product_ref && line.quantity != null ? formatEuro(lineTotal(line)) : "" }}</td>
              <td class="select-cell">
                <input
                  v-if="!isEntryLine(line)"
                  v-model="line.is_option"
                  type="checkbox"
                  aria-label="Ligne en option"
                />
              </td>
              <td class="num seuil" :class="{ warn: isBelowThreshold(line) }">
                {{ formatUnitPrice(line.threshold_price) }}
              </td>
              <td>
                <Button
                  v-if="line.product_ref"
                  icon="pi pi-times"
                  text
                  rounded
                  size="small"
                  severity="secondary"
                  @click="removeLine(line)"
                />
              </td>
            </tr>
          </template>
        </tbody>
      </table>
      <!-- Sous le tableau : ajout de lignes à gauche, totaux à droite. Les montants sont alignés
           sur la colonne « Total HT » (position mesurée, voir totalsStyle). -->
      <div class="lines-footer">
        <!-- À gauche : ajout de lignes, puis notes ; à droite : totaux. -->
        <div class="footer-left">
          <div class="add-lines">
            <Button
              v-tooltip.bottom="'Titre de paragraphe, ajouté en fin de devis, à déplacer avec la poignée'"
              label="Titre"
              icon="pi pi-bars"
              text
              size="small"
              @click="insertSpecialLine('title')"
            />
            <Button
              v-tooltip.bottom="'Ajoutée en fin de devis, à déplacer avec la poignée'"
              label="Texte"
              icon="pi pi-align-left"
              text
              size="small"
              @click="insertSpecialLine('text')"
            />
            <Button
              v-tooltip.bottom="'Additionne les articles depuis le sous-total précédent'"
              label="Sous-total"
              icon="pi pi-calculator"
              text
              size="small"
              @click="insertSpecialLine('subtotal')"
            />
            <Button
              v-tooltip.bottom="'Toujours en bas du devis, non remisés'"
              label="Frais de port"
              icon="pi pi-truck"
              text
              size="small"
              :disabled="hasFee('shipping')"
              @click="addFee('shipping')"
            />
            <Button
              v-tooltip.bottom="'Toujours en bas du devis, non remisés'"
              label="Frais de facturation"
              icon="pi pi-receipt"
              text
              size="small"
              :disabled="hasFee('billing')"
              @click="addFee('billing')"
            />
          </div>
          <div class="notes">
            <label for="notes" class="muted">Notes (imprimées sur le devis)</label>
            <Textarea id="notes" v-model="notes" rows="3" fluid auto-resize />
          </div>
        </div>

        <!-- Ordre du calcul : produits, remise (produits seulement), frais, total. -->
        <div ref="totalsPanel" class="totals" :style="totalsStyle">
          <span class="t-label">Total produits HT</span>
          <span></span>
          <span class="t-amount">{{ formatEuro(totals.products) }}</span>
          <span></span>

          <label v-tooltip.top="'Les frais de port et de facturation ne sont pas remisés'" class="t-label" for="global-discount">
            Remise sur les produits
          </label>
          <InputNumber
            v-model="globalDiscount"
            input-id="global-discount"
            :min="0"
            :max="100"
            :max-fraction-digits="2"
            suffix=" %"
            size="small"
            fluid
            input-class="num"
          />
          <span class="t-amount">{{ globalDiscount ? `− ${formatEuro(totals.discount)}` : "" }}</span>
          <span></span>

          <template v-if="globalDiscount">
            <span class="t-label strong">Total produits remisé HT</span>
            <span></span>
            <span class="t-amount strong">{{ formatEuro(round2(totals.products - totals.discount)) }}</span>
            <span></span>
          </template>

          <!-- Frais de port / de facturation : après la remise (jamais remisés), montant modifiable. -->
          <template v-for="fee in feeLines" :key="fee.key">
            <InputText v-model="fee.designation" size="small" fluid class="fee-label" />
            <span class="muted fee-hint">non remisé</span>
            <div class="euro-input">
              <InputText
                :id="`fee-${fee.key}`"
                :model-value="fee.priceText"
                inputmode="decimal"
                autocomplete="off"
                placeholder="0,00"
                size="small"
                fluid
                class="num"
                @update:model-value="(v: string | undefined) => { fee.auto = false; onPriceInput(fee, v ?? ''); }"
                @blur="onPriceBlur(fee)"
              />
              <span>€</span>
            </div>
            <Button
              v-tooltip.left="`Retirer les ${fee.designation.toLowerCase() || 'frais'}`"
              icon="pi pi-times"
              text
              rounded
              size="small"
              severity="secondary"
              class="t-remove"
              @click="removeFee(fee)"
            />
          </template>

          <span class="t-label grand">Total HT</span>
          <span></span>
          <span class="t-amount grand">{{ formatEuro(netTotal) }}</span>
          <span></span>

          <!-- Lignes en option : à part, hors total HT. -->
          <template v-if="lines.some((l) => l.is_option && l.product_ref)">
            <span class="t-label option">Total options HT</span>
            <span></span>
            <span class="t-amount option">{{ formatEuro(totals.options) }}</span>
            <span></span>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.draft-badge {
  font-size: 0.9em;
}

.won-toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 3px 10px 3px 6px;
  border: 1px solid var(--app-border);
  border-radius: 999px;
  font-size: 0.85rem;
  color: var(--app-muted);
  cursor: pointer;
  user-select: none;
}

.won-toggle.on {
  border-color: color-mix(in srgb, var(--app-success) 45%, transparent);
  background: color-mix(in srgb, var(--app-success) 10%, transparent);
  color: var(--app-success);
  font-weight: 600;
}

.versions {
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: 7px;
  background: color-mix(in srgb, var(--app-muted) 12%, transparent);
}

.version {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 2px 8px;
  border-radius: 5px;
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--app-muted);
  text-decoration: none;
}

.version:hover {
  color: var(--app-text);
}

.version.current {
  background: var(--app-surface);
  color: var(--app-text);
  box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
}

.version i {
  font-size: 0.65rem;
  color: var(--app-success);
}

.draft-badge i {
  font-size: 0.5rem;
  color: var(--app-warn);
  vertical-align: middle;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: 1.5rem;
  align-items: flex-start;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.field label {
  color: var(--app-muted);
}

.client-field {
  flex: 1;
}

.client-picker {
  display: flex;
  gap: 0.5rem;
}

.client-picker > :first-child {
  flex: 1;
}

/* Détails du client : une ligne par thème (légende à gauche, champs avec libellé au-dessus). */
/* Pleine largeur sous la recherche du client et la date (filet de séparation compris). */
.client-details {
  flex-basis: 100%;
  display: grid;
  grid-template-columns: 90px 1fr;
  column-gap: 1rem;
  row-gap: 0.4rem;
  align-items: start;
  padding-top: 1rem;
  border-top: 1px solid var(--app-border);
}

.cd-caption {
            <Button
              v-tooltip.bottom="
                cleanupCount
                  ? 'Supprime les articles sans quantité, les lignes vides, et les titres, textes et sous-totaux qui n\'accompagnent plus aucun article'
                  : 'Rien à nettoyer'
              "
              :label="cleanupCount ? `Nettoyer (${cleanupCount})` : 'Nettoyer'"
              icon="pi pi-eraser"
              text
              size="small"
              severity="secondary"
              :disabled="!cleanupCount"
              @click="cleanUpLines"
            />
  padding-top: 0.45rem;
  font-size: 0.75rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--app-muted);
}

.cd-fields {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem 1rem;
  align-items: flex-start;
}

/* Libellé devant le champ. */
.cd-fields .field {
  flex-direction: row;
  align-items: center;
  gap: 0.5rem;
}

.cd-fields .field label {
  font-size: 0.85rem;
  white-space: nowrap;
}

.cd-name {
  flex: 0 1 380px;
  font-weight: 600;
}

/* Contenu aligné sur la hauteur d'un champ (étiquettes, bouton). */
.cd-inline {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  flex-wrap: wrap;
  min-height: 2.1rem;
}

.cd-fields :deep(input.pct) {
  width: 90px;
  text-align: right;
}

.contact-name {
  width: 170px;
}

.contact-email {
  width: 220px;
}

.contact-phone {
  width: 150px;
}

.sales-rep {
  min-width: 200px;
}

/* Code de liste de prix cliquable : ouvre le tiroir des produits. */
.list-link {
  padding: 0;
  border: none;
  background: none;
  cursor: pointer;
}

.list-link:hover :deep(.p-tag) {
  background: color-mix(in srgb, var(--app-accent) 14%, transparent);
  color: var(--app-accent);
}

.list-select {
  min-width: 220px;
}

.hint {
  font-size: 0.85em;
}

.dialog-hint {
  margin-top: 0;
}

.opt {
  display: flex;
  gap: 0.75rem;
  justify-content: space-between;
  width: 100%;
}

/* Plein écran : la carte des lignes couvre toute la zone de contenu (la barre latérale reste
   visible), en-tête du tableau figé en haut. */
.lines-fullscreen {
  position: fixed;
  inset: 0 0 0 var(--sidebar-width, 0);
  transition: left 0.18s ease;
  z-index: 900;
  overflow: auto;
  padding-top: 0;
  border: none;
  border-radius: 0;
}

.lines-fullscreen .lines thead th {
  position: sticky;
  top: 0;
  z-index: 2;
}

.lines th.fullscreen-cell {
  padding: 0;
  vertical-align: middle;
  text-align: center;
}

.fullscreen-btn {
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
}

.fullscreen-btn:hover {
  background: color-mix(in srgb, var(--app-muted) 15%, transparent);
  color: var(--app-text);
}

.fullscreen-btn i {
  font-size: 0.75rem;
}

/* Lignes du devis : rendu « tableur » (quadrillage, champs sans bordure, lignes basses)
   pour voir une vingtaine d'articles d'un coup. */
.lines {
  width: 100%;
  table-layout: fixed;
  border-collapse: collapse;
  font-size: 0.8125rem;
}

.lines th {
  text-align: left;
  vertical-align: bottom;
  font-weight: 600;
  font-size: 0.75rem;
  line-height: 1.2;
  color: var(--app-muted);
  padding: 6px;
  background: color-mix(in srgb, var(--app-muted) 8%, var(--app-surface));
  border: 1px solid var(--app-border);
}

.lines th.num {
  text-align: right;
}

.lines td {
  height: 26px;
  padding: 0 6px;
  vertical-align: middle;
  border: 1px solid var(--app-border);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Cellule qui contient un champ : le champ occupe toute la cellule. */
.lines td:has(> .p-inputtext),
.lines td:has(> .euro-input),
.lines td:has(> .p-textarea),
.lines td:has(> .p-autocomplete) {
  padding: 0;
}

/* Montant saisi : le symbole € suit le champ, comme dans les autres colonnes de prix. */
.euro-input {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  padding-right: 6px;
}

.euro-input :deep(.p-inputtext) {
  flex: 1;
  min-width: 0;
}

.lines .euro-input :deep(.p-inputtext) {
  padding-right: 0;
}

/* Dans les totaux (champ encadré) : le € est dans le champ, pour rester aligné sur les montants. */
.totals .euro-input {
  position: relative;
  padding-right: 0;
}

.totals .euro-input :deep(.p-inputtext) {
  padding-right: 1.5rem;
}

.totals .euro-input span {
  position: absolute;
  right: 0.6rem;
  pointer-events: none;
}

/* Poignée et cases à cocher : centrées, sans « … » de débordement. */
.lines td.drag-cell,
.lines td.select-cell {
  padding: 0;
  text-overflow: clip;
}

/* Texte libre : zone multi-ligne, même apparence que les champs. */
.lines tbody :deep(.p-textarea) {
  display: block;
  width: 100%;
  min-height: 26px;
  padding: 4px 6px;
  font-size: 0.8125rem;
  line-height: 18px;
  border: none;
  border-radius: 0;
  background: transparent;
  box-shadow: none;
  white-space: pre-wrap;
}

.lines tbody :deep(.p-textarea:enabled:focus) {
  box-shadow: inset 0 0 0 2px var(--app-accent);
  background: var(--app-surface);
}

.lines tbody :deep(.p-inputtext) {
  width: 100%;
  height: 26px;
  padding: 0 6px;
  font-size: 0.8125rem;
  border: none;
  border-radius: 0;
  background: transparent;
  box-shadow: none;
}

.lines tbody :deep(.p-inputtext:enabled:focus) {
  box-shadow: inset 0 0 0 2px var(--app-accent);
  background: var(--app-surface);
}

.lines tbody :deep(.p-inputtext.p-invalid) {
  box-shadow: inset 0 0 0 1px var(--app-accent);
  background: color-mix(in srgb, var(--app-accent) 6%, transparent);
}

.lines tbody :deep(.p-inputtext::placeholder) {
  color: color-mix(in srgb, var(--app-muted) 60%, transparent);
}

.lines tbody :deep(.p-button.p-button-icon-only) {
  width: 22px;
  height: 22px;
}

.drag-cell {
  text-align: center;
}

/* Toute la cellule sert de poignée (l'icône seule était trop petite à attraper). */
.drag-handle {
  cursor: grab;
  user-select: none;
}

.drag-handle i {
  color: var(--app-muted);
  opacity: 0.5;
}

.drag-handle:hover {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
}

tr:hover .drag-handle i {
  opacity: 1;
}

/* Glisser-déposer : lignes déplacées estompées, trait vert à l'endroit du dépôt. */
/* Lignes en option : hors total, en italique avec un liseré. */
.lines :deep(.enedis-input) {
  font-size: 0.9em;
}

/* Prix remisé / LPN : en gris, sauf celui qui a fixé le PU HT. */
.lines td.ref-price {
  color: var(--app-muted);
}

.lines td.ref-price.used {
  color: var(--app-text);
  font-weight: 600;
}

.lines tr.option-row td {
  font-style: italic;
  background: color-mix(in srgb, var(--app-muted) 7%, transparent);
}

.lines tr.option-row td:first-child {
  box-shadow: inset 3px 0 0 var(--app-muted);
}

.lines tr.dragging td {
  opacity: 0.35;
}

.lines tr.drop-before td {
  box-shadow: inset 0 3px 0 var(--app-accent);
}

:global(body.dragging-lines),
:global(body.dragging-lines *) {
  cursor: grabbing !important;
  user-select: none;
}

/* Texte : noir, gras (taille 10–11 sur le PDF). */
.text-row :deep(.text-line) {
  font-weight: 600;
  color: var(--app-text);
  border-color: transparent;
  background: transparent;
  box-shadow: none;
}

.text-row :deep(.text-line:hover),
.text-row :deep(.text-line:focus) {
  border-color: var(--app-border);
}

/* Titre : gras, rouge, taille courante (taille 12 sur le PDF). */
.lines .title-row :deep(.title-line) {
  font-weight: 700;
  color: var(--app-accent);
  border-color: transparent;
  background: transparent;
  box-shadow: none;
}

.title-row :deep(.title-line:hover),
.title-row :deep(.title-line:focus) {
  border-color: var(--app-border);
}



.subtotal-row td {
  background: color-mix(in srgb, var(--app-accent) 9%, transparent);
  border-bottom: 2px solid color-mix(in srgb, var(--app-accent) 40%, transparent);
}

.subtotal-row :deep(.subtotal-label) {
  font-weight: 600;
  text-align: right;
  background: transparent;
}

.lines td.subtotal-amount {
  font-weight: 700;
  overflow: visible;
}

.fee-hint {
  font-size: 0.85em;
}

.add-lines {
  display: flex;
  gap: 0.25rem;
  white-space: nowrap;
}

.seuil {
  color: var(--app-muted);
  font-size: 0.92em;
}

/* Jamais en italique, même sur une ligne en option. */
.lines tr.option-row td.seuil {
  font-style: normal;
}

.selection-bar {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  margin: -0.25rem 0 0.75rem;
  padding: 0.25rem 0.5rem;
  border-radius: 8px;
  background: color-mix(in srgb, var(--app-accent) 8%, transparent);
}

.selection-bar strong {
  margin-right: 0.5rem;
}

.selection-bar .spacer {
  flex: 1;
}

.select-cell {
  text-align: center;
}

.select-cell input {
  margin: 0;
}

.select-cell input {
  accent-color: var(--app-accent);
  cursor: pointer;
}

.lines tr.selected td {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
}

.lines small {
  display: block;
  margin-top: 3px;
}

:deep(input.num) {
  text-align: right;
}

/* Sous le tableau : boutons d'ajout à gauche, panneau des totaux à droite. */
.lines-footer {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 2rem;
  margin-top: 0.75rem;
}

.footer-left {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.notes {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

/* Colonnes : libellé | saisie | montant (largeur et position de « Total HT ») | reste du tableau.
   Pas de marge à droite : la dernière colonne fait exactement la largeur restante du tableau. */
.totals {
  display: grid;
  align-items: center;
  column-gap: 0.75rem;
  row-gap: 0.5rem;
  padding: 1rem 12px 1rem 1.25rem;
  border: 1px solid var(--app-border);
  border-radius: 8px;
  background: color-mix(in srgb, var(--app-muted) 4%, var(--app-surface));
}

.t-label {
  text-align: right;
  color: var(--app-muted);
}

.t-amount {
  padding-right: 6px;
  white-space: nowrap;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.totals .strong {
  font-weight: 700;
  color: var(--app-text);
}

.totals :deep(.fee-label) {
  text-align: right;
}

.t-remove {
  justify-self: center;
}

/* Total HT final (remise et frais compris) : grand, en rouge, un peu détaché. */
.totals .grand {
  padding-top: 0.4rem;
  font-size: 1.15rem;
  font-weight: 700;
  color: var(--app-accent);
}

.totals .option {
  font-style: italic;
}
</style>
  flex-wrap: wrap;
