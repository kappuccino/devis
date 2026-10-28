<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import AutoComplete, { type AutoCompleteCompleteEvent } from "primevue/autocomplete";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import Button from "primevue/button";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import Dialog from "primevue/dialog";
import RadioButton from "primevue/radiobutton";
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
  type QuoteTemplate,
  type QuoteVersion,
} from "../api";
import { errorMessage, formatDate, formatEuro, MOD, round2, todayIso } from "../format";
import { exportQuotePdf } from "../composables/usePdf";
import QuoteDocsDialog from "../components/docs/QuoteDocsDialog.vue";
import PriceListDrawer from "../components/PriceListDrawer.vue";
import ClientFormDialog from "../components/ClientFormDialog.vue";
import { readDraft, removeDraft, writeDraft, type QuoteDraft } from "../drafts";
import { useConfirm } from "primevue/useconfirm";
import { useLineClipboard } from "../composables/useLineClipboard";
import {
  cleanLines,
  FEE_KINDS,
  insertBlock,
  isFee,
  moveBlockBefore,
  quoteTotals,
  subtotals,
  type FeeKind,
} from "../quoteLines";
import { favoriteLists } from "../favorites";
import { lastSalesRep, rememberSalesRep, salesRepList } from "../salesReps";
import { applyColumnTabOrder } from "../gridNav";
import QuoteLineRow from "../components/QuoteLineRow.vue";
import { CUSTOM_SOURCE, editText, isCustom, LINE_ROW_ACTIONS, type EditLine } from "../editLine";
import {
  autoFeeChanges,
  feeRulesFrom,
  parseRates,
  ratesFor,
  SHIPPING_RATES_KEY,
  shippingSuggestionRange,
  type FeeRules,
  type ShippingRate,
} from "../fees";

/** `template` : éditeur d'un devis type (sans client, ni prix, ni totaux, ni PDF). */
const props = defineProps<{ id?: string; template?: boolean }>();
const isTemplate = computed(() => !!props.template);
/** Nom du devis type. */
const templateName = ref("");
/** Nom de l'affaire (chantier, opération…), imprimé sur le PDF. */
const projectName = ref("");
const router = useRouter();
const route = useRoute();
const toast = useToast();
const confirm = useConfirm();

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
  price_locked: false,
  hide_price: false,
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
  scheduleQtyPrice(line);
}

function onPriceInput(line: EditLine, text: string) {
  line.priceText = text;
  line.unit_price = parseNumber(text) ?? 0;
}

/** PU calculé par l'appli (prix de liste ou prix public remisé), et non saisi à la main. */
const isAutoPrice = (l: EditLine) =>
  l.price_source != null &&
  ((l.price_source === l.lpn_list && l.unit_price === l.lpn_price) ||
    (l.price_source.startsWith("Public") && l.unit_price === l.discounted_price));

/** Recalcul du prix en cours de frappe de la quantité, après une courte pause (par ligne). */
const QTY_PRICE_DELAY = 250;
const qtyTimers = new Map<number, ReturnType<typeof setTimeout>>();
function scheduleQtyPrice(line: EditLine) {
  clearTimeout(qtyTimers.get(line.key));
  qtyTimers.set(
    line.key,
    setTimeout(() => {
      qtyTimers.delete(line.key);
      priceForQuantity(line);
    }, QTY_PRICE_DELAY),
  );
}
onBeforeUnmount(() => qtyTimers.forEach((t) => clearTimeout(t)));

/** Quantité validée (champ quitté) : texte remis en forme, prix recalculé sans attendre. */
function onQtyCommit(line: EditLine) {
  line.qtyText = editText(line.quantity, 3);
  if (!qtyTimers.has(line.key)) return;
  clearTimeout(qtyTimers.get(line.key));
  qtyTimers.delete(line.key);
  priceForQuantity(line);
}

/**
 * Quantité modifiée : les prix de liste dépendent de la quantité (prix dégressifs dès une quantité
 * mini). Le PU est recalculé s'il vient de l'appli ; un prix saisi à la main et la désignation
 * restent tels quels.
 */
async function priceForQuantity(line: EditLine) {
  if (line.kind !== "item" || !line.product_ref || line.pendingRef != null) return;
  if (isTemplate.value || !hasClient.value) return;
  // Réf refusée (ex. prix de liste seulement dès 10 pièces) : on retente avec la nouvelle quantité.
  if (line.error) {
    line.resolvedRef = null;
    await resolve(line);
    return;
  }
  if (line.price_locked || !isAutoPrice(line)) return;
  const quantity = line.quantity;
  try {
    const p = await api.resolvePrice(pricing.value, line.product_ref, quantity);
    // Quantité retouchée entre-temps, ou prix saisi à la main pendant le calcul.
    if (line.quantity !== quantity || line.price_locked || !isAutoPrice(line)) return;
    Object.assign(line, {
      unit_price: p.unit_price,
      priceText: editText(p.unit_price, 2),
      price_source: p.source,
      discounted_price: p.discounted_price,
      lpn_price: p.lpn_price,
      lpn_list: p.lpn_list,
      lpnTiers: p.lpn_tiers,
      lpnMinQty: p.lpn_min_qty,
    });
  } catch {
    // Produit disparu entre-temps : on garde le prix de la ligne.
  }
}

/**
 * Prix saisi sur une ligne à référence libre (vide : pas de prix). Le PU HT suit le prix remisé,
 * à défaut le prix public, tant qu'il n'a pas été saisi à la main.
 */
function onCustomPrice(line: EditLine, field: "public_price" | "discounted_price" | "threshold_price", text: string) {
  const auto = line.discounted_price ?? line.public_price ?? 0;
  const follows = line.unit_price === auto;
  const v = parseNumber(text);
  line[field] = text.trim() && v != null ? round2(v) : null;
  if (follows && field !== "threshold_price") {
    line.unit_price = line.discounted_price ?? line.public_price ?? 0;
    line.priceText = line.unit_price ? editText(line.unit_price, 2) : "";
  }
}

/** Remise supplémentaire de la ligne, en % (jusqu'à 100 ; négative pour une augmentation). */
function onDiscountInput(line: EditLine, text: string) {
  line.discountText = text;
  // Négative : augmentation du prix (ex. −5 → +5 %).
  line.discount = Math.min(100, parseNumber(text) ?? 0);
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
/** Ligne à qui sont destinées les propositions (les autres reçoivent NO_HITS et ne bougent pas). */
const productSuggestionsFor = ref<number | null>(null);
const NO_HITS: ProductHit[] = [];
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
  // Franco de port atteint : les frais de port suggérés (non modifiés depuis) n'ont plus lieu d'être.
  const range = shippingRange.value;
  if (range && totals.value.products >= range.to) feeLines.value = feeLines.value.filter((l) => !l.suggested);
}

// ---------- Frais de port suggérés (camion), entre le minimum de facturation et le franco ----------

const shippingRange = ref<{ from: number; to: number } | null>(null);
const shippingRates = ref<ShippingRate[]>([]);
const showTruck = computed(() => {
  const range = shippingRange.value;
  const products = totals.value.products;
  return !isTemplate.value && range != null && products >= range.from && products < range.to;
});
const shippingVisible = ref(false);
/** Département de livraison tapé (gardé d'une ouverture à l'autre). */
const shippingDepartment = ref("");
// Du moins cher au plus cher : le moins cher est proposé d'office (ex. « 4 » avant « 4 VILLES HAUTES MONTAGNES »).
const shippingMatches = computed(() =>
  ratesFor(shippingRates.value, shippingDepartment.value).sort((a, b) => a.price - b.price),
);
const shippingChoice = ref<ShippingRate | null>(null);
// Un seul tarif : choisi d'office ; plusieurs (ex. « villes hautes montagnes ») : le moins cher.
watch(shippingMatches, (m) => (shippingChoice.value = m[0] ?? null));

function openShipping() {
  shippingChoice.value = shippingMatches.value[0] ?? null;
  shippingVisible.value = true;
}

/** Ajoute les frais de port suggérés, ou remplace le montant des frais de port déjà présents. */
function acceptShipping() {
  const rate = shippingChoice.value;
  if (!rate) return;
  if (!hasFee("shipping")) addFee("shipping", { amount: rate.price });
  const fee = feeLines.value.find((l) => l.kind === "shipping")!;
  Object.assign(fee, { auto: false, suggested: true, unit_price: rate.price, priceText: editText(rate.price, 2) });
  shippingVisible.value = false;
}

/** Remise globale sur les produits, en % (les frais ne sont pas remisés). */
const globalDiscount = ref(0);
/** Total HT (produits hors options + frais), remise, total remisé et total des options. */
const totals = computed(() => quoteTotals([...lines.value, ...feeLines.value], globalDiscount.value));
// Frais automatiques : seulement sur les modifications du devis, pas à son chargement.
watch(
  () => totals.value.products,
  () => {
    if (draftReady.value && !isTemplate.value) applyAutoFees();
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
 * Navigation dans la colonne Qté ou PU HT, lignes de texte et sous-totaux sautés :
 * - Entrée / ↓ : même colonne de l'article suivant ; après le dernier, Entrée revient à la saisie de réf ;
 * - ↑ : même colonne de l'article précédent.
 */
function moveInColumn(line: EditLine, column: "qty" | "price", direction: 1 | -1, e?: Event) {
  e?.preventDefault();
  if (column === "qty") line.qtyText = editText(line.quantity, 3);
  const items = lines.value.filter((l) => l.kind === "item" && !isEntryLine(l));
  const target = items[items.indexOf(line) + direction];
  let input = target ? document.getElementById(`${column}-${target.key}`) : null;
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
/** addToBase : le client est ajouté aux clients (code P0001…) au lieu de n'exister que dans ce devis. */
const ephemeral = ref({ name: "", cfa: 0, cfo: 0, list: null as string | null, addToBase: true });
const priceListOptions = computed(() =>
  allPriceLists.value.map((l) => ({ code: l.code, label: `${l.code}${l.label ? ` — ${l.label}` : ""}` })),
);

function openEphemeral(name?: string) {
  ephemeral.value = {
    name: name ?? (clientCode.value ? lastClientQuery.value : clientName.value || lastClientQuery.value),
    cfa: discountCfa.value,
    cfo: discountCfo.value,
    list: !clientCode.value ? (priceLists.value[0] ?? null) : null,
    addToBase: true,
  };
  ephemeralVisible.value = true;
}

async function applyEphemeral() {
  const { name, cfa, cfo, list, addToBase } = ephemeral.value;
  if (!name.trim()) {
    toast.add({ severity: "warn", summary: "Indiquez la raison sociale", life: 2500 });
    return;
  }
  if (addToBase) {
    const c = await createOneOffClient(name, cfa ?? 0, cfo ?? 0, list);
    if (!c) return;
    ephemeralVisible.value = false;
    client.value = c;
    await applyClient(c);
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

/** Ajoute un client ponctuel à la base (code P0001…) ; null en cas d'erreur (message affiché). */
async function createOneOffClient(name: string, cfa: number, cfo: number, list: string | null) {
  try {
    const c = await api.createOneOffClient(name.trim(), cfa, cfo, list);
    clients.value.push(c);
    return c;
  } catch (e) {
    toast.add({ severity: "error", summary: "Ajout du client", detail: errorMessage(e) });
    return null;
  }
}

/**
 * Client hors base de ce devis (ancien client ponctuel) ajouté aux clients, avec les conditions du
 * devis. Les devis enregistrés au même nom (celui-ci compris) lui sont rattachés.
 */
async function addCurrentClientToBase() {
  const name = clientName.value.trim();
  const c = await createOneOffClient(name, discountCfa.value ?? 0, discountCfo.value ?? 0, priceLists.value[0] ?? null);
  if (!c) return;
  client.value = c;
  clientCode.value = c.code;
  let linked = 0;
  try {
    linked = await api.linkQuotesToClient(c.code, name);
  } catch (e) {
    toast.add({ severity: "error", summary: "Rattachement des devis", detail: errorMessage(e) });
  }
  toast.add({
    severity: "success",
    summary: `Client ${c.code} ajouté aux clients`,
    detail: linked ? `${linked} devis enregistré${linked > 1 ? "s" : ""} au nom « ${name} » rattaché${linked > 1 ? "s" : ""}` : undefined,
    life: 4000,
  });
}

// Fiche du client de la base (remises, listes, commentaire), modifiable depuis le devis.
const clientFormVisible = ref(false);
/** Client de la base du devis ; null pour un client hors base ou absent de ce poste. */
const clientRecord = computed(() => (clientCode.value ? (clients.value.find((c) => c.code === clientCode.value) ?? null) : null));

/** Fiche enregistrée : ses remises et listes passent dans le devis, et les prix sont recalculés. */
async function onClientSaved(saved: Client) {
  const i = clients.value.findIndex((c) => c.code === saved.code);
  const previous = i >= 0 ? clients.value[i] : null;
  if (i >= 0) clients.value[i] = saved;
  if (client.value?.code === saved.code) client.value = saved;
  if (previous && clientName.value.trim() === previous.name.trim()) clientName.value = saved.name;
  discountCfa.value = saved.discount_cfa;
  discountCfo.value = saved.discount_cfo;
  priceLists.value = [...saved.price_lists];
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
  const th = totalHeader.value?.getBoundingClientRect();
  // Bord droit du contenu du pied (cadre des totaux), qui peut ne pas tomber sur celui du tableau.
  const footer = totalsPanel.value?.parentElement;
  if (!th || !th.width || !footer) return;
  const style = getComputedStyle(footer);
  const right = footer.getBoundingClientRect().right - parseFloat(style.paddingRight) - parseFloat(style.borderRightWidth);
  totalColumn.value = { width: th.width, tail: right - th.right };
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
  if (totalsPanel.value?.parentElement) tableObserver.observe(totalsPanel.value.parentElement);
  measureTotalColumn();
});
onBeforeUnmount(() => tableObserver?.disconnect());

// Tab descend dans la colonne, Maj+Tab remonte. Les tabindex sont posés juste avant que Tab ne
// déplace le curseur (écoute en phase de capture), et non après chaque rendu : à chaque frappe,
// cela parcourait toutes les cellules du tableau (lent sur les gros devis).
const updateTabOrder = () => applyColumnTabOrder(linesTable.value, totalsPanel.value);
const onTabKey = (e: KeyboardEvent) => {
  if (e.key === "Tab") updateTabOrder();
};
onMounted(() => {
  updateTabOrder();
  window.addEventListener("keydown", onTabKey, true);
});
onBeforeUnmount(() => window.removeEventListener("keydown", onTabKey, true));

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
  productSuggestionsFor.value = line.key;
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
const enedisSuggestionsFor = ref<number | null>(null);
const isStaleEnedis = (line: EditLine) =>
  line.pendingRef != null || document.activeElement?.id !== `enedis-${line.key}`;
async function searchEnedis(e: AutoCompleteCompleteEvent, line: EditLine) {
  enedisSuggestionsFor.value = line.key;
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
  if (field === "enedis" && isCustom(line) && line.product_ref) line.resolvedEnedis = value;
  else {
    if (field === "enedis") line.product_ref = value;
    await resolve(line, false, field);
  }
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
  // Devis type : pas de client, la réf est seulement vérifiée (prix non affichés).
  if (!hasClient.value && !isTemplate.value) {
    line.error = "Choisissez d'abord un client";
    return;
  }
  line.pendingRef = r;
  // Une autre saisie a été lancée entre-temps (ex. choix dans la liste ENEDIS) : on l'ignore.
  const stale = () => line.pendingRef !== r;
  try {
    const p = await api.resolvePrice(pricing.value, r, line.quantity);
    if (stale()) return;
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
      lpnTiers: p.lpn_tiers,
      lpnMinQty: p.lpn_min_qty,
      resolvedRef: p.product_ref,
      resolvedEnedis: p.enedis_code,
      error: null,
    });
  } catch (e) {
    if (stale()) return;
    const message = errorMessage(e);
    // Réf inconnue : référence libre, dont on saisit les colonnes à la main. Déjà libre, elle garde
    // ses valeurs (recalcul des prix, réf corrigée) ; venue d'un produit, ses prix sont vidés.
    if (/introuvable/.test(message)) {
      const wasCustom = isCustom(line);
      // PU calculé pour l'ancien produit : sans objet pour la nouvelle réf (un PU saisi est gardé).
      if (!wasCustom && isAutoPrice(line)) Object.assign(line, { unit_price: 0, priceText: "" });
      Object.assign(line, {
        resolvedRef: r,
        price_source: CUSTOM_SOURCE,
        lpn_price: null,
        lpn_list: null,
        lpnTiers: undefined,
        lpnMinQty: null,
        error: null,
        ...(wasCustom
          ? {}
          : {
              public_price: null,
              threshold_price: null,
              discounted_price: null,
              enedis_code: field === "enedis" ? r : null,
              resolvedEnedis: field === "enedis" ? r : null,
            }),
      });
    } else {
      Object.assign(line, {
        resolvedRef: r,
        price_source: null,
        public_price: null,
        threshold_price: null,
        discounted_price: null,
        lpn_price: null,
        lpn_list: null,
        error: message,
      });
    }
  } finally {
    if (!stale()) line.pendingRef = null;
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
  // Produit en cours de résolution (choix dans la liste) : rien à refaire.
  if (line.pendingRef != null) return;
  // Référence libre : le code ENEDIS se saisit librement, sans changer la réf.
  if (unchanged || (isCustom(line) && line.product_ref)) {
    line.resolvedEnedis = code;
    if (focusNext && !line.error) focusNewLine("enedis");
    return;
  }
  line.product_ref = code;
  resolve(line, focusNext, "enedis");
}

/**
 * Choix d'un produit dans la liste de la colonne ENEDIS : on prend sa référence exacte
 * (un même code ENEDIS peut être porté par plusieurs produits).
 */
function selectEnedis(line: EditLine, hit: ProductHit) {
  line.enedis_code = hit.enedis_code;
  line.product_ref = hit.ref;
  resolve(line, true, "enedis");
}

/**
 * Recalcule les prix avec les conditions du devis.
 * `onlyPublic` : seulement les lignes au prix public remisé (après un changement de remise),
 * pour ne pas écraser les prix de liste ni les prix saisis à la main.
 */
async function repriceLines(onlyPublic = false) {
  // Prix verrouillés (colonne « Verrouiller prix ») : gardés tels quels.
  const candidates = filledLines.value.filter((l) => !onlyPublic || l.price_source?.startsWith("Public"));
  const priced = candidates.filter((l) => !l.price_locked);
  const locked = candidates.length - priced.length;
  if (!priced.length && !locked) return;
  // La réf est déjà connue : on force un nouveau calcul.
  priced.forEach((l) => (l.resolvedRef = null));
  await Promise.all(priced.map((l) => resolve(l)));
  const detail = locked ? `${locked} prix verrouillé${locked > 1 ? "s" : ""} gardé${locked > 1 ? "s" : ""}` : undefined;
  toast.add({ severity: "info", summary: "Prix recalculés", detail, life: 2500 });
}

/** En-tête de la colonne au cadenas : verrouiller (ou déverrouiller) toutes les lignes avec une réf. */
const allLocked = computed(() => filledLines.value.length > 0 && filledLines.value.every((l) => l.price_locked));
const someLocked = computed(() => filledLines.value.some((l) => l.price_locked));
function toggleAllLocked() {
  const lock = !allLocked.value;
  filledLines.value.forEach((l) => (l.price_locked = lock));
}

/** En-tête de la colonne à l'œil : masquer (ou afficher) le prix de toutes les lignes sur le PDF. */
const allHidden = computed(() => filledLines.value.length > 0 && filledLines.value.every((l) => l.hide_price));
const someHidden = computed(() => filledLines.value.some((l) => l.hide_price));
function toggleAllHidden() {
  const hide = !allHidden.value;
  filledLines.value.forEach((l) => (l.hide_price = hide));
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

// Sélection à la souris, sur une partie de ligne qui n'est pas un champ (prix affichés, total,
// marges…) : clic → cette ligne seule, Ctrl/⌘+clic → ajoute ou retire la ligne, Maj+clic → plage
// depuis le clic précédent, clic glissé → toutes les lignes survolées.
const NOT_SELECTING = "input, textarea, button, select, a, .drag-handle, .p-autocomplete, .p-inputtext";
const rangeSelect = ref<{ anchor: number; base: Set<number> } | null>(null);

function onRowPointerDown(line: EditLine, e: PointerEvent) {
  if (e.button !== 0 || isEntryLine(line)) return;
  if ((e.target as Element | null)?.closest(NOT_SELECTING)) return;
  e.preventDefault();
  (document.activeElement as HTMLElement | null)?.blur?.();
  const add = e.ctrlKey || e.metaKey;
  if (e.shiftKey && lastClickedKey != null && selectableLines.value.some((l) => l.key === lastClickedKey)) {
    rangeSelect.value = { anchor: lastClickedKey, base: add ? new Set(selectedKeys.value) : new Set() };
    selectRangeTo(line.key);
  } else if (add) {
    const keys = new Set(selectedKeys.value);
    if (keys.has(line.key)) keys.delete(line.key);
    else keys.add(line.key);
    selectedKeys.value = keys;
    lastClickedKey = line.key;
    rangeSelect.value = { anchor: line.key, base: new Set(keys) };
  } else {
    selectedKeys.value = new Set([line.key]);
    lastClickedKey = line.key;
    rangeSelect.value = { anchor: line.key, base: new Set() };
  }
  document.body.classList.add("selecting-lines");
  window.addEventListener("pointermove", onRangeMove);
  window.addEventListener("pointerup", stopRangeSelect);
  window.addEventListener("pointercancel", stopRangeSelect);
}

/** Sélection de la ligne d'ancrage jusqu'à `key` (en plus des lignes déjà là au Ctrl/⌘+clic). */
function selectRangeTo(key: number) {
  const r = rangeSelect.value;
  if (!r) return;
  const all = selectableLines.value;
  const a = all.findIndex((l) => l.key === r.anchor);
  const b = all.findIndex((l) => l.key === key);
  if (a < 0 || b < 0) return;
  const keys = new Set(r.base);
  all.slice(Math.min(a, b), Math.max(a, b) + 1).forEach((l) => keys.add(l.key));
  selectedKeys.value = keys;
}

function onRangeMove(e: PointerEvent) {
  if (!rangeSelect.value || !tbodyEl.value) return;
  const rows = Array.from(tbodyEl.value.querySelectorAll<HTMLElement>("tr[data-key]"));
  // Au-dessus de la première ligne ou sous la dernière : on s'arrête à la ligne du bord.
  const row =
    rows.find((r) => e.clientY < r.getBoundingClientRect().bottom) ?? rows[rows.length - 1];
  if (row) selectRangeTo(Number(row.dataset.key));
  const scroller = tbodyEl.value.closest(".content");
  if (scroller) {
    const box = scroller.getBoundingClientRect();
    if (e.clientY < box.top + 48) scroller.scrollBy(0, -16);
    else if (e.clientY > box.bottom - 48) scroller.scrollBy(0, 16);
  }
}

function stopRangeSelect() {
  rangeSelect.value = null;
  document.body.classList.remove("selecting-lines");
  window.removeEventListener("pointermove", onRangeMove);
  window.removeEventListener("pointerup", stopRangeSelect);
  window.removeEventListener("pointercancel", stopRangeSelect);
}
onBeforeUnmount(stopRangeSelect);

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
    const items = lines.value.filter((l) => pastedKeys.has(l.key) && l.kind === "item" && !l.price_locked);
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

function removeLine(line: EditLine) {
  lines.value = lines.value.filter((l) => l !== line);
  if (!lines.value.length) lines.value.push(blankLine());
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
  price_locked: !!l.price_locked,
  hide_price: !!l.hide_price,
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
    is_template: isTemplate.value,
    template_name: templateName.value.trim(),
    project_name: projectName.value.trim(),
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
  if (isTemplate.value && !templateName.value.trim()) {
    toast.add({ severity: "warn", summary: "Donnez un nom au devis type", life: 2500 });
    document.getElementById("template-name")?.focus();
    return null;
  }
  if (!hasClient.value && !isTemplate.value) {
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
    if (isNew) removeDraft(newDraftKey());
    markSaved();
    if (isNew) router.replace(`${basePath.value}/${saved.id}`);
    toast.add({
      severity: "success",
      summary: isTemplate.value ? `Devis type « ${saved.template_name} » enregistré` : `Devis ${saved.number} enregistré`,
      life: 2000,
    });
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
      if (result.warning) toast.add({ severity: "warn", summary: "Conditions générales de vente", detail: result.warning });
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
      !isCustom(l) &&
      (l.public_price == null || (l.discounted_price == null && l.lpn_price == null)),
  );
  await Promise.all(
    missing.map(async (l) => {
      try {
        const p = await api.resolvePrice(ctx, l.product_ref, l.quantity);
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

/** Brouillon d'un devis (ou devis type) pas encore enregistré. */
const newDraftKey = () => (isTemplate.value ? "type-nouveau" : "nouveau");
const draftKey = () => (quoteId.value == null ? newDraftKey() : String(quoteId.value));
/** Liste et éditeur : devis ou devis types. */
const basePath = computed(() => (isTemplate.value ? "/devis-types" : "/devis"));
/** Faux pendant un chargement : on n'écrit pas de brouillon d'un état à moitié chargé. */
const draftReady = ref(false);
/** État tel qu'en base (ou vierge) : tant qu'on y est, pas de brouillon. */
let savedSnapshot = "";
const hasDraft = ref(false);

/** Ce qui compte pour savoir si le devis a changé (pas le prix public / seuil complétés en tâche de fond). */
const snapshot = () =>
  JSON.stringify({
    client: [clientCode.value, clientName.value, pricingKey.value, contact.value],
    templateName: templateName.value,
    projectName: projectName.value,
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
      !!l.price_locked,
      !!l.hide_price,
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
    template_name: templateName.value,
    project_name: projectName.value,
    savedAt: new Date().toISOString(),
  };
  writeDraft(draftKey(), draft);
  hasDraft.value = true;
}

let draftTimer: ReturnType<typeof setTimeout> | undefined;
watch([clientCode, clientName, pricingKey, contact, date, notes, globalDiscount, lines, templateName, projectName], () => {
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
  templateName.value = draft.template_name ?? templateName.value;
  projectName.value = draft.project_name ?? projectName.value;
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
      removeDraft(newDraftKey());
      if (quoteId.value) router.push(`${basePath.value}/nouveau`);
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
      templateName.value = "";
      projectName.value = "";
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
    templateName.value = q.template_name ?? "";
    projectName.value = q.project_name ?? "";
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
    if (!isTemplate.value) fillMissingReferencePrices();
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement du devis", detail: errorMessage(e) });
  }
}

// Le composant est réutilisé entre /devis/nouveau et /devis/:id (bouton « Nouveau devis », duplication).
// Après la création d'un devis, l'URL passe à /devis/:id sans recharger : quoteId est déjà à jour.
watch(
  () => [props.id, props.template] as const,
  ([id], [, wasTemplate]) => {
    // Autre devis (ou nouveau) : on sort du plein écran.
    fullscreen.value = false;
    if (props.template !== wasTemplate || id !== (quoteId.value == null ? undefined : String(quoteId.value))) {
      load(id).then(applyTemplateFromQuery);
    }
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
    shippingRange.value = shippingSuggestionRange(settings);
    shippingRates.value = parseRates(settings[SHIPPING_RATES_KEY]);
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des clients", detail: errorMessage(e) });
  }
  await load(props.id);
  await applyTemplateFromQuery();
});

// ---------- Devis types ----------

/** Devis types proposés dans un nouveau devis (« Partir d'un devis type »). */
const templates = ref<QuoteTemplate[]>([]);
watch(
  () => quoteId.value == null && !isTemplate.value,
  async (isNewQuote) => {
    if (!isNewQuote) return;
    try {
      templates.value = await api.listQuoteTemplates();
    } catch {
      templates.value = [];
    }
  },
  { immediate: true },
);

/** Nouveau devis ouvert avec `?type=<id>` (onglet Devis types) : on reprend ce devis type. */
async function applyTemplateFromQuery() {
  const id = Number(route.query.type);
  if (!id || quoteId.value != null || isTemplate.value) return;
  router.replace({ query: {} });
  askApplyTemplate(id);
}

/** Reprise d'un devis type : confirmation si le devis a déjà des lignes (elles sont gardées). */
function askApplyTemplate(id: number) {
  const existing = lines.value.filter((l) => !isEntryLine(l)).length;
  if (!existing) {
    applyTemplate(id);
    return;
  }
  const name = templates.value.find((t) => t.id === id)?.name ?? "";
  confirm.require({
    header: "Reprendre un devis type",
    message:
      `Ce devis contient déjà ${existing} ligne(s). Les lignes du devis type${name ? ` « ${name} »` : ""} ` +
      "seront ajoutées à la suite (les lignes actuelles sont gardées).",
    icon: "pi pi-bookmark",
    rejectProps: { label: "Annuler", severity: "secondary", text: true },
    acceptProps: { label: "Ajouter les lignes" },
    accept: () => applyTemplate(id),
  });
}

/**
 * Nouveau devis : reprend les lignes et les notes d'un devis type (sans prix), ajoutées à la suite
 * des lignes déjà saisies ; les prix sont calculés pour le client du devis (dès qu'il est choisi).
 */
async function applyTemplate(id: number) {
  try {
    const t = await api.getQuote(id);
    const added: EditLine[] = t.lines
      .filter((l) => !isFee(l.kind))
      .map((l) => ({
        ...blankLine(),
        ...withTexts({
          ...l,
          quantity: l.kind === "item" && !l.quantity ? null : l.quantity,
          unit_price: 0,
          price_locked: false,
        }),
        resolvedRef: null,
      }));
    lines.value = [...lines.value.filter((l) => !isEntryLine(l)), ...added, blankLine()];
    if (t.notes && !notes.value.trim()) notes.value = t.notes;
    toast.add({
      severity: "info",
      summary: `Devis type « ${t.template_name} » repris`,
      detail: hasClient.value ? undefined : "Choisissez le client : les prix seront calculés pour lui.",
      life: 3000,
    });
    if (hasClient.value) await repriceLines();
  } catch (e) {
    toast.add({ severity: "error", summary: "Devis type", detail: errorMessage(e) });
  }
}

/** « Enregistrer comme devis type » : nom demandé, puis ouverture du devis type créé. */
const asTemplateVisible = ref(false);
const asTemplateName = ref("");
function openSaveAsTemplate() {
  asTemplateName.value = "";
  asTemplateVisible.value = true;
}
async function saveAsTemplate() {
  const name = asTemplateName.value.trim();
  if (!name || quoteId.value == null) return;
  if (hasDraft.value && !(await save())) return;
  try {
    const t = await api.saveAsTemplate(quoteId.value, name, todayIso());
    asTemplateVisible.value = false;
    toast.add({ severity: "success", summary: `Devis type « ${t.template_name} » créé`, life: 2500 });
    router.push(`/devis-types/${t.id}`);
  } catch (e) {
    toast.add({ severity: "error", summary: "Devis type", detail: errorMessage(e) });
  }
}

// Actions appelées par les lignes du tableau (QuoteLineRow) : fournies une fois, elles ne changent
// jamais, et une ligne n'est redessinée que si elle-même change.
provide(LINE_ROW_ACTIONS, {
  startDrag,
  onRowPointerDown,
  onSelectClick,
  removeLine,
  searchEnedis,
  selectEnedis,
  commitEnedis,
  searchProducts,
  commitRef,
  onRefBlur,
  validateAndGoToQuantity,
  onQtyInput,
  onQtyCommit,
  onPriceInput,
  onPriceBlur,
  onDiscountInput,
  onCustomPrice,
  moveInColumn,
});
</script>

<template>
  <div class="page">
    <div class="page-header">
      <Button icon="pi pi-arrow-left" text rounded @click="router.push(isTemplate ? '/devis?onglet=types' : '/devis')" />
      <h1 v-if="isTemplate">{{ quoteId != null ? `Devis type ${templateName}` : "Nouveau devis type" }}</h1>
      <h1 v-else>{{ number ? `Devis ${number}` : "Nouveau devis" }}</h1>
      <!-- Versions du devis : liens de l'une à l'autre, la version obtenue cochée. -->
      <nav v-if="versions.length > 1 && !isTemplate" class="versions" aria-label="Versions du devis">
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
        v-if="quoteId != null && !isTemplate"
        v-tooltip.bottom="wonAt ? `Cochée le ${formatDate(wonAt)}` : 'Le devis est devenu une commande / facture'"
        class="won-toggle"
        :class="{ on: wonAt }"
      >
        <Checkbox :model-value="!!wonAt" binary @update:model-value="setWon" />
        Affaire obtenue
      </label>
      <!-- Modifications non enregistrées : pastille seule, explication en infobulle. -->
      <span
        v-if="hasDraft"
        v-tooltip.bottom="'Modifications non enregistrées (gardées sur cet ordinateur ; reprises si vous revenez sur ce devis)'"
        class="draft-badge"
        role="status"
        aria-label="Modifications non enregistrées"
      >
        <i class="pi pi-circle-fill" />
      </span>
      <span class="spacer" />
      <Select
        v-if="quoteId == null && !isTemplate && templates.length"
        :options="templates"
        option-label="name"
        option-value="id"
        placeholder="Partir d'un devis type…"
        size="small"
        class="template-select"
        aria-label="Partir d'un devis type"
        @update:model-value="(id: number) => askApplyTemplate(id)"
      />
      <Button
        v-if="quoteId != null && !isTemplate"
        v-tooltip.bottom="'Ses lignes et notes, sans client ni prix, pour démarrer d\'autres devis'"
        icon="pi pi-bookmark"
        severity="secondary"
        text
        rounded
        aria-label="Enregistrer comme devis type"
        @click="openSaveAsTemplate"
      />
      <Button
        v-if="quoteId != null && !isTemplate"
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
      <Button v-if="!isTemplate" label="PDF" icon="pi pi-file-pdf" :loading="saving" @click="savePdf" />
      <Button
        v-if="!isTemplate"
        v-tooltip.bottom="'Devis suivi de la documentation technique des produits'"
        label="PDF + Docs"
        icon="pi pi-book"
        :loading="saving"
        @click="savePdfWithDocs"
      />
    </div>
    <QuoteDocsDialog v-model:visible="docsDialogVisible" :quote="docsDialogQuote" />
    <PriceListDrawer v-model:code="drawerList" />
    <Dialog v-model:visible="shippingVisible" modal header="Frais de port" :style="{ width: '440px' }">
      <p v-if="!shippingRates.length" class="muted dialog-hint">
        Aucun tableau des frais de port : importez-le dans Réglages → Frais automatiques.
      </p>
      <form v-else class="shipping-form" @submit.prevent="acceptShipping">
        <label for="shipping-department">Département de livraison</label>
        <InputText
          id="shipping-department"
          v-model="shippingDepartment"
          autofocus
          autocomplete="off"
          placeholder="ex. 33, 04, 2A ou code postal"
        />
        <div v-if="shippingMatches.length > 1" class="shipping-choices">
          <label v-for="(r, i) in shippingMatches" :key="i" class="shipping-choice">
            <RadioButton v-model="shippingChoice" :value="r" name="shipping-rate" />
            <span>{{ r.department }} — {{ r.destination }}</span>
            <span class="num">{{ formatEuro(r.price) }}</span>
          </label>
        </div>
        <p v-if="shippingChoice" class="shipping-result">
          Frais de port suggérés ({{ shippingChoice.destination || shippingChoice.department }}) :
          <strong>{{ formatEuro(shippingChoice.price) }}</strong>
        </p>
        <p v-else-if="shippingDepartment.trim()" class="warn">Département introuvable dans le tableau.</p>
      </form>
      <template #footer>
        <template v-if="shippingChoice">
          <span class="shipping-question">Souhaitez-vous ajouter ces frais de port ?</span>
          <Button label="Non" severity="secondary" text @click="shippingVisible = false" />
          <Button label="Oui" icon="pi pi-check" @click="acceptShipping" />
        </template>
        <Button v-else label="Fermer" severity="secondary" text @click="shippingVisible = false" />
      </template>
    </Dialog>

    <Dialog v-model:visible="asTemplateVisible" modal header="Enregistrer comme devis type" :style="{ width: '440px' }">
      <p class="muted dialog-hint">
        Les lignes et les notes de ce devis, sans client ni prix : les prix seront calculés pour le client de
        chaque nouveau devis.
      </p>
      <form class="form-grid" @submit.prevent="saveAsTemplate">
        <label for="as-template-name">Nom</label>
        <InputText id="as-template-name" v-model="asTemplateName" autofocus placeholder="ex. Lotissement – 24 lots" />
      </form>
      <template #footer>
        <Button label="Annuler" severity="secondary" text @click="asTemplateVisible = false" />
        <Button label="Créer le devis type" icon="pi pi-bookmark" :disabled="!asTemplateName.trim()" @click="saveAsTemplate" />
      </template>
    </Dialog>

    <!-- Devis type : un nom, pas de client. -->
    <div v-if="isTemplate" class="card template-head">
      <label for="template-name">Nom du devis type</label>
      <InputText id="template-name" v-model="templateName" placeholder="ex. Lotissement – 24 lots" class="template-name" />
      <span class="muted">Sans client ni prix : les prix sont calculés pour le client de chaque devis créé à partir de ce type.</span>
    </div>

    <div v-else class="card head">
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
                <span class="opt-code">
                  <Tag v-if="option.is_one_off" value="Ponctuel" severity="info" class="small-tag" />
                  <span class="muted mono">{{ option.code }}</span>
                </span>
              </div>
            </template>
            <template #empty>
              <span class="muted">Aucun client trouvé — « Client ponctuel » pour un client hors base</span>
            </template>
          </AutoComplete>
          <Button
            v-tooltip.bottom="'Client hors fichier LPN : ajouté aux clients, ou seulement dans ce devis'"
            label="Client ponctuel"
            icon="pi pi-user-plus"
            severity="secondary"
            outlined
            @click="openEphemeral()"
          />
        </div>

        <Dialog v-model:visible="ephemeralVisible" modal header="Client ponctuel" :style="{ width: '480px' }">
          <p class="muted dialog-hint">
            {{
              ephemeral.addToBase
                ? "Ce client est ajouté aux clients (tag « Ponctuel ») : on le retrouve dans la recherche pour les prochains devis."
                : "Ce client n'est pas ajouté aux clients : il n'existe que dans ce devis."
            }}
          </p>
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
            <span />
            <div class="eph-add">
              <Checkbox v-model="ephemeral.addToBase" input-id="eph-add" binary />
              <label for="eph-add">Ajouter aux clients</label>
            </div>
            <button type="submit" hidden />
          </form>
          <template #footer>
            <Button label="Annuler" severity="secondary" text @click="ephemeralVisible = false" />
            <Button label="Valider" icon="pi pi-check" @click="applyEphemeral" />
          </template>
        </Dialog>
        <ClientFormDialog v-model:visible="clientFormVisible" :client="clientRecord" @saved="onClientSaved" />
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
              <Tag v-if="clientRecord?.is_one_off" value="Ponctuel" severity="info" />
              <Button
                v-if="clientRecord"
                v-tooltip.bottom="
                  clientRecord.comment
                    ? { value: clientRecord.comment, class: 'comment-tooltip' }
                    : 'Remises, listes de prix et commentaire du client'
                "
                label="Fiche client"
                :icon="clientRecord.comment ? 'pi pi-comment' : 'pi pi-id-card'"
                severity="secondary"
                text
                size="small"
                @click="clientFormVisible = true"
              />
              <template v-if="!clientCode">
                <Tag value="Client ponctuel" severity="info" />
                <Button
                  v-tooltip.bottom="'Ajoute ce client aux clients et lui rattache les devis enregistrés à son nom'"
                  label="Ajouter aux clients"
                  icon="pi pi-user-plus"
                  severity="secondary"
                  text
                  size="small"
                  @click="addCurrentClientToBase"
                />
              </template>
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
          <!-- Nom de l'affaire : à droite de la ligne client ; imprimé sur le PDF. -->
          <div class="field cd-project">
            <label for="project-name">Affaire</label>
            <InputText
              id="project-name"
              v-model="projectName"
              v-tooltip.bottom="'Nom de l\'affaire (chantier, opération…), imprimé sur le devis'"
              placeholder="Nom de l'affaire"
              size="small"
              class="cd-project-input"
            />
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

    <!-- Lignes hors cadre (toute la largeur de la page) ; boutons, notes et totaux dans un cadre. -->
    <div class="lines-area" :class="{ 'lines-fullscreen': fullscreen }">
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
            <template v-if="!isTemplate">
              <th style="width: 76px" class="num">Prix public</th>
              <th v-tooltip.top="'Prix public − remise CFA / CFO du devis'" style="width: 76px" class="num">Prix remisé</th>
              <th v-tooltip.top="'Prix négocié de la liste de prix'" style="width: 76px" class="num">LPN</th>
              <th style="width: 80px" class="num">PU HT</th>
              <th style="width: 34px" class="select-cell">
                <button
                  v-tooltip.top="
                    allLocked
                      ? 'Prix verrouillés (gardés quand les prix sont recalculés). Clic : tout déverrouiller'
                      : 'Verrouiller le prix de toutes les lignes (gardé quand les prix sont recalculés, ex. en changeant de liste forcée)'
                  "
                  type="button"
                  class="col-toggle"
                  :class="{ on: allLocked, partial: someLocked && !allLocked }"
                  :aria-pressed="allLocked"
                  :disabled="!filledLines.length"
                  aria-label="Verrouiller le prix de toutes les lignes"
                  @click="toggleAllLocked"
                >
                  <i :class="someLocked ? 'pi pi-lock' : 'pi pi-lock-open'" />
                </button>
              </th>
              <th v-tooltip.top="'Remise supplémentaire sur le prix de la ligne ; négative pour une augmentation (ex. −5 = +5 %)'" style="width: 72px" class="num">
                Rem. sup.
              </th>
              <th ref="totalHeader" style="width: 100px" class="num">Total HT</th>
            </template>
            <th v-tooltip.top="'Ligne en option : hors total HT, comptée dans « Total options »'" style="width: 30px" class="select-cell narrow-head">
              Opt.
            </th>
            <th v-if="!isTemplate" style="width: 34px" class="select-cell">
              <button
                v-tooltip.top="
                  allHidden
                    ? 'Prix masqués sur le PDF (lignes comptées dans les totaux). Clic : tout afficher'
                    : 'Masquer le prix de toutes les lignes sur le PDF (elles restent comptées dans les totaux)'
                "
                type="button"
                class="col-toggle"
                :class="{ on: allHidden, partial: someHidden && !allHidden }"
                :aria-pressed="allHidden"
                :disabled="!filledLines.length"
                aria-label="Masquer le prix de toutes les lignes sur le PDF"
                @click="toggleAllHidden"
              >
                <i :class="someHidden ? 'pi pi-eye-slash' : 'pi pi-eye'" />
              </button>
            </th>
            <th v-if="!isTemplate" v-tooltip.top="'Non imprimé sur le devis'" style="width: 70px" class="num seuil">Prix seuil</th>
            <th style="width: 26px" class="remove-cell"></th>
          </tr>
        </thead>
        <tbody ref="tbodyEl">
          <QuoteLineRow
            v-for="line in lines"
            :key="line.key"
            :line="line"
            :is-template="isTemplate"
            :is-entry="isEntryLine(line)"
            :selected="selectedKeys.has(line.key)"
            :dragging="!!drag?.moved && drag.keys.has(line.key)"
            :drop-before="dropKey === line.key"
            :subtotal="line.kind === 'subtotal' ? subtotalAmounts.get(line.key) : undefined"
            :product-suggestions="productSuggestionsFor === line.key ? productSuggestions : NO_HITS"
            :enedis-suggestions="enedisSuggestionsFor === line.key ? enedisSuggestions : NO_HITS"
          />
        </tbody>
      </table>
      <!-- Juste sous les lignes, hors du cadre : ajout de titres, textes, sous-totaux, frais. -->
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
          v-if="!isTemplate"
          v-tooltip.bottom="'Toujours en bas du devis, non remisés'"
          label="Frais de port"
          icon="pi pi-truck"
          text
          size="small"
          :disabled="hasFee('shipping')"
          @click="addFee('shipping')"
        />
        <Button
          v-if="!isTemplate"
          v-tooltip.bottom="'Toujours en bas du devis, non remisés'"
          label="Frais de facturation"
          icon="pi pi-receipt"
          text
          size="small"
          :disabled="hasFee('billing')"
          @click="addFee('billing')"
        />
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
      </div>
      <!-- Cadre du bas : notes à gauche, totaux à droite. Les montants sont alignés sur la colonne
           « Total HT » (position mesurée, voir totalsStyle). -->
      <div class="card lines-footer">
        <!-- À gauche : notes ; à droite : totaux. -->
        <div class="footer-left">
          <div class="notes">
            <Textarea id="notes" v-model="notes" rows="4" fluid auto-resize class="notes-input" />
            <label for="notes" class="muted">Notes (imprimées sur le devis)</label>
          </div>
        </div>

        <!-- Ordre du calcul : produits, remise (produits seulement), frais, total. -->
        <div v-if="!isTemplate" ref="totalsPanel" class="totals" :style="totalsStyle">
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

          <!-- Entre le minimum de facturation et le franco, sans frais de port : ligne « Frais de port »
               avec le camion qui clignote (frais de port suggérés selon le département). -->
          <template v-if="showTruck && !hasFee('shipping')">
            <div class="fee-name">
              <Button
                v-tooltip.left="'Calculer les frais de port'"
                icon="pi pi-truck"
                text
                rounded
                size="small"
                aria-label="Calculer les frais de port"
                class="truck"
                :class="{ blink: !hasFee('shipping') }"
                @click="openShipping"
              />
              <span class="muted fee-placeholder">Frais de port</span>
            </div>
            <span></span>
            <span></span>
            <span></span>
          </template>

          <!-- Frais de port / de facturation : après la remise (jamais remisés), montant modifiable. -->
          <template v-for="fee in feeLines" :key="fee.key">
            <div class="fee-name">
              <template v-if="fee.kind === 'shipping' && showTruck">
                <Button
                  v-tooltip.left="'Calculer les frais de port'"
                  icon="pi pi-truck"
                  text
                  rounded
                  size="small"
                  aria-label="Calculer les frais de port"
                  class="truck"
                  :class="{ blink: !hasFee('shipping') }"
                  @click="openShipping"
                />
              </template>
              <InputText v-model="fee.designation" size="small" fluid class="fee-label" />
            </div>
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
                @update:model-value="(v: string | undefined) => { fee.auto = false; fee.suggested = false; onPriceInput(fee, v ?? ''); }"
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
  display: inline-flex;
  padding: 4px;
  cursor: help;
}

.template-select {
  min-width: 200px;
}

/* Devis type : son nom à la place du client. */
.template-head {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.template-head label {
  font-weight: 600;
}

.template-name {
  width: 360px;
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
  font-size: 0.6rem;
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
  /* Plus proche du sélecteur de client que l'écart normal entre les blocs (1,5 rem). */
  margin-top: -0.75rem;
  display: grid;
  grid-template-columns: 90px 1fr;
  column-gap: 1rem;
  row-gap: 0.4rem;
  align-items: start;
}

.cd-caption {
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

/* Nom de l'affaire : poussé à droite de la ligne client. */
.cd-project {
  margin-left: auto;
}

.cd-project-input {
  width: 320px;
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
  width: 60px;
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

.opt-code {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.small-tag {
  font-size: 0.7rem;
  padding: 0.05rem 0.4rem;
}

.eph-add {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.eph-add label {
  color: inherit;
}

/* Plein écran : la carte des lignes couvre toute la zone de contenu (la barre latérale reste
   visible), en-tête du tableau figé en haut. */
.lines-fullscreen {
  position: fixed;
  inset: 0 0 0 var(--sidebar-width, 0);
  transition: left 0.18s ease;
  z-index: 900;
  overflow: auto;
  padding: 0 1rem 1rem;
  background: var(--app-surface);
}

.lines-fullscreen .lines thead th {
  position: sticky;
  top: 0;
  z-index: 2;
}

.lines th.fullscreen-cell {
  padding: 0;
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
  vertical-align: middle;
  font-weight: 600;
  font-size: 0.75rem;
  line-height: 1.2;
  color: var(--app-muted);
  padding: 6px;
  background: color-mix(in srgb, var(--app-muted) 8%, var(--app-surface));
  border: 1px solid var(--app-border);
}

/* En-tête court (« Opt. ») : colonne réduite à son libellé. */
.lines th.narrow-head {
  padding-left: 2px;
  padding-right: 2px;
}

.lines th.num {
  text-align: right;
}

.lines :deep(td) {
  height: 26px;
  padding: 0 6px;
  vertical-align: middle;
  border: 1px solid var(--app-border);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Cellule qui contient un champ : le champ occupe toute la cellule. */
.lines :deep(td:has(> .p-inputtext)),
.lines :deep(td:has(> .euro-input)),
.lines :deep(td:has(> .p-textarea)),
.lines :deep(td:has(> .p-autocomplete)) {
  padding: 0;
}

/* Référence libre (hors catalogue) : réf en italique orangé, seulement dans l'appli. */
.lines :deep(.custom-ref) {
  color: var(--app-warn);
  font-style: italic;
}

/* Montant saisi : le symbole € suit le champ, comme dans les autres colonnes de prix. */
.euro-input,
.lines :deep(.euro-input) {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  padding-right: 6px;
}

.euro-input :deep(.p-inputtext),
.lines :deep(.euro-input .p-inputtext) {
  flex: 1;
  min-width: 0;
}

.lines :deep(.euro-input .p-inputtext) {
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
.lines :deep(td.drag-cell),
.lines :deep(td.select-cell),
.lines :deep(td.remove-cell) {
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

.lines :deep(.drag-cell),
.lines :deep(.remove-cell) {
  text-align: center;
}

/* Toute la cellule sert de poignée (l'icône seule était trop petite à attraper). */
.lines :deep(.drag-handle) {
  cursor: grab;
  user-select: none;
}

.lines :deep(.drag-handle i) {
  color: var(--app-muted);
  opacity: 0.5;
}

.lines :deep(.drag-handle:hover) {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
}

.lines :deep(tr:hover .drag-handle i) {
  opacity: 1;
}

/* Glisser-déposer : lignes déplacées estompées, trait vert à l'endroit du dépôt. */
/* Lignes en option : hors total, en italique avec un liseré. */
.lines :deep(.enedis-input) {
  font-size: 0.9em;
}

/* Prix remisé / LPN : en gris, sauf celui qui a fixé le PU HT. */
.lines :deep(td.ref-price) {
  color: var(--app-muted);
}

.lines :deep(td.ref-price.used) {
  color: var(--app-text);
  font-weight: 600;
}

.lines :deep(tr.option-row td) {
  font-style: italic;
  background: color-mix(in srgb, var(--app-muted) 7%, transparent);
}

.lines :deep(tr.option-row td:first-child) {
  box-shadow: inset 3px 0 0 var(--app-muted);
}

.lines :deep(tr.dragging td) {
  opacity: 0.35;
}

.lines :deep(tr.drop-before td) {
  box-shadow: inset 0 3px 0 var(--app-accent);
}

:global(body.dragging-lines),
:global(body.dragging-lines *) {
  cursor: grabbing !important;
  user-select: none;
}

:global(body.selecting-lines),
:global(body.selecting-lines *) {
  user-select: none;
}

/* En-têtes cadenas / œil : l'icône seule, qui coche ou décoche toute la colonne. Ouvert (gris) :
   aucune ligne ; fermé estompé : une partie ; fermé en couleur : toutes. */
.col-toggle {
  vertical-align: middle;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
}

.col-toggle:hover:not(:disabled) {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
}

.col-toggle:disabled {
  opacity: 0.4;
  cursor: default;
}

.col-toggle.partial {
  color: color-mix(in srgb, var(--app-accent) 55%, var(--app-muted));
}

.col-toggle.on {
  color: var(--app-accent);
}

.col-toggle .pi {
  font-size: 0.85rem;
}

/* Texte : noir, gras (taille 10–11 sur le PDF). */
.lines :deep(.text-row .text-line) {
  font-weight: 600;
  color: var(--app-text);
  border-color: transparent;
  background: transparent;
  box-shadow: none;
}

.lines :deep(.text-row .text-line:hover),
.lines :deep(.text-row .text-line:focus) {
  border-color: var(--app-border);
}

/* Titre : gras, rouge, taille courante (taille 12 sur le PDF). */
.lines :deep(.title-row .title-line) {
  font-weight: 700;
  /* Noir comme sur le PDF. */
  color: var(--app-text);
  border-color: transparent;
  background: transparent;
  box-shadow: none;
}

.lines :deep(.title-row .title-line:hover),
.lines :deep(.title-row .title-line:focus) {
  border-color: var(--app-border);
}



.lines :deep(.subtotal-row td) {
  background: color-mix(in srgb, var(--app-accent) 9%, transparent);
  border-bottom: 2px solid color-mix(in srgb, var(--app-accent) 40%, transparent);
}

.lines :deep(.subtotal-row .subtotal-label) {
  font-weight: 600;
  text-align: right;
  background: transparent;
}

.lines :deep(td.subtotal-amount) {
  font-weight: 700;
  overflow: visible;
}

/* Camion des frais de port suggérés : clignote tant qu'aucun frais de port n'est sur le devis. */
.truck.p-button {
  color: var(--app-accent);
}

.truck.blink {
  animation: truck-blink 1.2s ease-in-out infinite;
}

@keyframes truck-blink {
  50% {
    opacity: 0.25;
  }
}

.shipping-form {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.shipping-choices {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.shipping-choice {
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: 0.5rem;
  cursor: pointer;
}

.shipping-result {
  margin: 0.25rem 0 0;
}

.shipping-question {
  margin-right: auto;
}

.fee-hint {
  font-size: 0.85em;
}

/* Boutons d'ajout : à partir de la 3e colonne (ENEDIS), après la poignée et la case de sélection
   (2 × 26 px). */
.add-lines {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
  white-space: nowrap;
  margin-top: 0.4rem;
  padding-left: 52px;
}

.seuil,
.lines :deep(.seuil) {
  color: var(--app-muted);
  font-size: 0.92em;
}

/* Jamais en italique, même sur une ligne en option. */
.lines :deep(tr.option-row td.seuil) {
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

.select-cell,
.lines :deep(.select-cell) {
  text-align: center;
}

.select-cell input,
.lines :deep(.select-cell input) {
  margin: 0;
}

.select-cell input,
.lines :deep(.select-cell input) {
  accent-color: var(--app-accent);
  cursor: pointer;
}

.lines :deep(tr.selected td) {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
}

.lines :deep(small) {
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

/* Lignes hors du cadre : fond des cellules comme dans un cadre. */
.lines-area .lines {
  background: var(--app-surface);
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

/* Hauteur de 4 lignes et demie (rows n'accepte que des lignes entières) : padding et bordures
   du champ en plus. Le champ s'agrandit ensuite avec le texte. */
.notes :deep(.notes-input) {
  min-height: calc(4.5lh + 1rem + 2px);
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

/* Libellé d'un frais, précédé du camion pour les frais de port. */
.fee-name {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 0.25rem;
}

.fee-placeholder {
  padding-right: 0.75rem;
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
