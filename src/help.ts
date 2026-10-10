// Page Aide : liste des fonctionnalités de l'appli, par rubrique.
// À TENIR À JOUR : chaque nouvelle fonctionnalité (ou modification visible) y est ajoutée
// (voir CLAUDE.md). `MOD` : ⌘ sur Mac, Ctrl sous Windows.
import { MOD } from "./format";

export interface HelpItem {
  title: string;
  text: string;
  /** Raccourcis clavier, affichés en touches. */
  keys?: string[];
}

export interface HelpSection {
  id: string;
  title: string;
  icon: string;
  items: HelpItem[];
}

export const HELP: HelpSection[] = [
  {
    id: "devis",
    title: "Devis",
    icon: "pi pi-file-edit",
    items: [
      {
        title: "Créer un devis",
        text: "« Nouveau devis » dans le menu. Choisissez un client de la base (recherche par nom ou code) ou un « Client ponctuel » qui n'existe que dans ce devis. Les remises CFA / CFO et les listes de prix du client sont copiées dans le devis et restent modifiables.",
      },
      {
        title: "Nom de l'affaire",
        text: "Champ « Affaire » à droite de la ligne Client (chantier, opération…). Il est imprimé sur le PDF au-dessus des lignes, à gauche du client, et apparaît sous le client dans la liste des devis (recherche possible).",
      },
      {
        title: "Saisie des lignes",
        text: "Tapez une référence ou un code ENEDIS : des propositions s'affichent. Le prix est calculé pour le client : le plus bas de ses listes de prix (LPN), sinon le prix public moins sa remise CFA ou CFO. Les colonnes Prix public, Prix remisé et LPN montrent d'où vient le prix (en gras : celui retenu).",
      },
      {
        title: "Navigation au clavier",
        text: "Tab descend dans la colonne, Maj+Tab remonte. Après une référence ou un code ENEDIS, Tab valide la ligne et passe à sa quantité. Entrée ou flèches haut / bas dans les quantités : ligne suivante / précédente.",
        keys: ["Tab", "Maj+Tab", "Entrée", "↑", "↓"],
      },
      {
        title: "Prix, remises et prix seuil",
        text: "Le PU HT reste modifiable. Remise supplémentaire possible par ligne, et remise globale sur les produits (les frais ne sont jamais remisés). Une ligne sous le prix seuil est signalée en rouge ; le prix seuil n'est jamais imprimé.",
      },
      {
        title: "Liste de prix forcée",
        text: "Le menu « Liste forcée » (listes favorites des Réglages) impose une liste de prix prioritaire pour tout le devis ; les prix sont recalculés aussitôt. Un clic sur le code d'une liste de prix ouvre le détail de ses produits.",
      },
      {
        title: "Titres, textes et sous-totaux",
        text: "Boutons sous les lignes : Titre (paragraphe en rouge), Texte (ligne libre), Sous-total (somme des articles depuis le sous-total précédent). Ils s'ajoutent en fin de devis ; déplacez-les avec la poignée.",
      },
      {
        title: "Lignes en option",
        text: "Case « Option » : la ligne n'est pas comptée dans le total HT ni dans les sous-totaux, mais dans « Total options HT », à part.",
      },
      {
        title: "Frais de port et de facturation",
        text: "Toujours en bas du devis, jamais remisés. Ajoutés automatiquement selon les seuils des Réglages (franco de port, minimum de facturation), sauf si vous les avez modifiés ou retirés.",
      },
      {
        title: "Sélection, copier, couper, coller, déplacer",
        text: "Cochez des lignes (Maj+clic : toute une plage), puis copiez, coupez ou supprimez-les ; collez après la dernière ligne sélectionnée ou en fin de devis, y compris dans un autre devis. Glissez une ligne (ou la sélection) par sa poignée pour la déplacer. Échap vide la sélection.",
        keys: [`${MOD}C`, `${MOD}X`, `${MOD}V`, "Échap"],
      },
      {
        title: "Nettoyer",
        text: "Le bouton « Nettoyer (n) » supprime en une fois les articles sans quantité, les lignes vides, et les titres, textes et sous-totaux qui n'accompagnent plus aucun article (après confirmation).",
      },
      {
        title: "Plein écran",
        text: "Bouton en haut à gauche du tableau : les lignes occupent toute la zone de travail (le menu reste visible). Échap ou le même bouton pour sortir ; changer de devis en sort aussi.",
        keys: ["Échap"],
      },
      {
        title: "Brouillon automatique",
        text: "Une pastille orange à côté du titre signale des modifications non enregistrées : elles sont gardées sur l'ordinateur et reprises si vous revenez sur le devis (même après avoir fermé l'appli). « Repartir de zéro » les abandonne.",
      },
      {
        title: "PDF et PDF + Docs",
        text: "« PDF » enregistre le devis puis génère le PDF (logo et coordonnées du rédacteur en haut, client dans un cadre gris en dessous, conditions générales de vente en bas du devis — identiques pour tous, intégrées à l'appli —, siège et adresse des commandes en pied de page, version complète des conditions en dernière page si choisie dans les Réglages) et l'ouvre. « PDF + Docs » ajoute à la suite la documentation technique des produits du devis (pages trouvées dans la documentation et fichiers ajoutés à la main), avec un sommaire.",
      },
      {
        title: "Versions d'un devis",
        text: "« Nouvelle version » copie le devis sous le même numéro suffixé -V2, -V3… Les pastilles V1 / V2 / V3 de l'en-tête passent d'une version à l'autre ; dans la liste, les anciennes versions sont estompées.",
      },
      {
        title: "Affaire obtenue",
        text: "Cochez « Affaire obtenue » (dans l'en-tête du devis ou dans la liste des devis) quand le devis devient une commande. Une seule version d'un devis peut l'être. Sert au filtre de la liste et aux Statistiques.",
      },
      {
        title: "Liste des devis",
        text: "Recherche par numéro ou client, filtre « Affaires obtenues / En attente », et pour chaque devis : PDF, nouvelle version, dupliquer (nouveau devis, nouveau numéro), supprimer.",
      },
      {
        title: "Numérotation",
        text: "Année sur 2 chiffres, préfixe et compteur : 26-JMOS-0001. Le compteur repart à 1 chaque année et peut prendre la suite des devis faits hors de l'appli (Réglages → Config PDF → « Dernier n° déjà utilisé »).",
      },
    ],
  },
  {
    id: "types",
    title: "Devis types",
    icon: "pi pi-bookmark",
    items: [
      {
        title: "Principe",
        text: "Un devis type est un modèle nommé (ex. « Lotissement – 24 lots ») : titres, articles, quantités habituelles, textes et notes, sans client ni prix. Onglet « Devis types » de l'écran Devis.",
      },
      {
        title: "Créer un devis type",
        text: "« Nouveau devis type » dans l'onglet, ou depuis un devis enregistré : bouton marque-page « Enregistrer comme devis type » (lignes et notes reprises, sans client, prix ni frais). Même éditeur que les devis, sans colonnes de prix.",
      },
      {
        title: "Partir d'un devis type",
        text: "« Créer un devis » dans l'onglet, ou le menu « Partir d'un devis type… » d'un nouveau devis. Les lignes du type sont ajoutées à la suite (confirmation demandée si le devis a déjà des lignes). Les prix sont calculés pour le client du devis, dès qu'il est choisi. Mettez à 0 ce qui ne sert pas, puis « Nettoyer ».",
      },
    ],
  },
  {
    id: "produits",
    title: "Produits",
    icon: "pi pi-box",
    items: [
      {
        title: "Catalogue",
        text: "Tous les produits importés du fichier LPN : référence, code ENEDIS, désignation, famille CFA / CFO, prix public, prix seuil, éco-taxe. Recherche par référence, désignation ou code ENEDIS.",
      },
      {
        title: "Évolution du chiffrage",
        text: "Bouton sur chaque produit : le prix réellement devisé de cette référence dans tous les devis (date, client, quantité, prix), avec un graphique et un filtre par client.",
      },
    ],
  },
  {
    id: "clients",
    title: "Clients",
    icon: "pi pi-users",
    items: [
      {
        title: "Fiches clients",
        text: "Clients importés du fichier LPN : code, raison sociale, groupe, commercial, remises CFA / CFO et listes de prix rattachées, modifiables (un réimport du fichier LPN les remplace).",
      },
      {
        title: "Export Excel des prix d'un client",
        text: "Bouton « Exporter ses prix (Excel) » : la liste de prix choisie, écrite dans le gabarit Excel des Réglages (logo, titre, coordonnées du client et du contact).",
      },
    ],
  },
  {
    id: "listes",
    title: "Listes de prix",
    icon: "pi pi-list",
    items: [
      {
        title: "Listes de prix nets (LPN)",
        text: "Toutes les listes du fichier LPN, avec leurs produits (prix net, prix public, remise) et leurs clients. Au-delà de 5 clients, « Afficher les n clients » ouvre un tiroir pour les filtrer, en rattacher ou en détacher.",
      },
    ],
  },
  {
    id: "stats",
    title: "Statistiques",
    icon: "pi pi-chart-bar",
    items: [
      {
        title: "Chiffres de l'année",
        text: "Par année (ou toutes) : nombre de devis, montant total et moyen, clients, affaires obtenues et taux de transformation, montant obtenu ; devis par mois (part obtenue en vert) ; meilleurs clients, commerciaux et produits les plus chiffrés. Montants HT après remise globale, frais compris, options exclues ; un devis compte une fois, quel que soit le nombre de versions.",
      },
    ],
  },
  {
    id: "documentation",
    title: "Documentation",
    icon: "pi pi-book",
    items: [
      {
        title: "Recherche dans la documentation technique",
        text: "Tapez une référence produit : les pages des PDF qui la contiennent et les images dont le nom la contient s'affichent, avec un aperçu. Exportez une page ou un fichier, ou glissez-le vers une autre application (mail…).",
        keys: ["↑", "↓"],
      },
      {
        title: "Indexation",
        text: "Le dossier de documentation (Réglages → Documentation) est indexé en tâche de fond à chaque démarrage : seuls les fichiers nouveaux ou modifiés sont relus.",
      },
    ],
  },
  {
    id: "reglages",
    title: "Réglages",
    icon: "pi pi-cog",
    items: [
      {
        title: "Import de données",
        text: "Importer le fichier LPN (xlsx) : produits, clients et listes de prix sont remplacés ; les devis ne sont jamais touchés.",
      },
      {
        title: "Favoris",
        text: "Listes de prix favorites, proposées dans chaque devis comme « Liste forcée ». Un clic sur le code d'une liste affiche ses produits.",
      },
      {
        title: "Config PDF",
        text: "Société et logo, rédacteur du devis (nom, adresse, téléphone, email, imprimés sous le logo), version complète des conditions générales de vente (PDF ajouté en dernière page ; le texte en bas du devis est fixe), préfixe et dernier numéro de devis, commerciaux, gabarit de l'export Excel, frais automatiques (franco de port, minimum de facturation).",
      },
      {
        title: "Sauvegarde",
        text: "Copies datées de la base des devis : automatiques au démarrage (au plus une par jour), à la demande, n dernières gardées, dans le dossier choisi. Restauration d'une copie (la base actuelle est d'abord copiée).",
      },
      {
        title: "Interface et mises à jour",
        text: "Thème clair, sombre ou automatique. Les nouvelles versions sont proposées au démarrage (Windows) et s'installent en un clic ; « À propos » donne la version.",
      },
    ],
  },
  {
    id: "general",
    title: "Général",
    icon: "pi pi-desktop",
    items: [
      {
        title: "Barre latérale",
        text: "Le bouton en haut de la barre latérale la replie (icônes seules) ou la déplie ; l'état est gardé d'un lancement à l'autre.",
        keys: [`${MOD}B`],
      },
      {
        title: "Fichiers et sécurité",
        text: "L'appli ne lit, n'écrit et n'ouvre que les fichiers choisis dans ses boîtes de dialogue (ou enregistrés dans les Réglages). Les données restent sur l'ordinateur.",
      },
    ],
  },
];
