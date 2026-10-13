# À faire

- [x] Changer la couleur de toute l'app et du PDF à partir du logo Cahors
  (logo : https://www.groupe-cahors.fr/sites/default/files/logo_0.png)

- [x] Lignes en option
  - Sur chaque ligne produit, une case à cocher « Option ».
  - Une ligne en option n'est **pas** comptée dans le total HT (ni dans les sous-totaux).
  - Elle est additionnée dans une ligne **« Total options »**, tout en bas du devis.
  - Usage : proposer un produit complémentaire que le client pourrait acheter en plus.
  - À prévoir aussi sur le PDF.

- [x] Listes de prix favorites et liste forcée sur un devis
  - Réglages : choisir plusieurs listes de prix (LPN) favorites, et pouvoir en retirer.
  - Devis : menu déroulant des listes favorites pour **forcer** une liste de prix.
  - La liste forcée est **prioritaire** sur les autres listes du devis.
  - Les prix sont recalculés à la volée quand on choisit, change ou retire la liste forcée.
  - Le devis mémorise la liste forcée, même si elle est retirée des favoris ensuite.

- [x] Contact du client sur le devis
  - Sous la raison sociale du client : champs **nom du contact**, **email** et **téléphone**.
  - Propres au devis, et imprimés sur le PDF dans le bloc client.

- [x] Nouvelle numérotation des devis : `26-ABCD-0001`, `26-ABCD-0002`…
  - Année sur 2 chiffres, puis préfixe (Réglages), puis compteur sur 4 chiffres.
  - Aujourd'hui : `ABCD-2026-0001`. Décider si les devis existants sont renumérotés ou gardent leur numéro.

- [x] Commercial du client sur le PDF
  - Le commercial rattaché au client (colonne « commercial » du fichier LPN) apparaît sur le PDF généré.
  - À préciser : que mettre pour un client ponctuel (aucun commercial, ou champ à saisir) ?

- [x] Conditions (CGV) en bas du devis
  - Texte à reprendre (exemple fourni) :
    > **VIREMENT SUR FACTURE 30 JOURS FIN DE MOIS LE 10**
    > Garantie contractuelle : 1 an, pièces et main d'œuvre en nos ateliers
    > Nos prix sont nets, unitaires, Hors Taxes.
    > Emballage et transport : Franco de port et d'emballage pour toute commande de plus de 840 EUR HT et livraison en un seul point.
    > Minimum de facturation : Toute commande inférieure à 140 EUR HT sera majorée de 25 EUR pour participation aux frais de facturation et de traitement.
    >
    > <u>Toute commande entraîne de plein droit l'acceptation de nos conditions générales de vente.</u>
    > (Ci-joint en dernière page.)
  - Mise en forme : 1re ligne en titre (gras, plus grand), mention d'acceptation soulignée.
  - « Ci-joint en dernière page » : ajouter les **CGV complètes en dernière page** du PDF (texte ou PDF fourni à part).
  - Remarque : le franco (840 € HT) et le minimum de facturation (140 € HT → +25 €) pourraient pré-remplir automatiquement les frais de port / de facturation.

- [x] Colonnes de prix : prix public, prix remisé, prix LPN
  - **Prix public** : déjà présent.
  - **Prix remisé** (nouvelle colonne) : prix public − remise CFA ou CFO du client, selon la famille du produit.
  - **LPN** (nouvelle colonne) : prix négocié de la liste de prix, vide si le produit n'est dans aucune liste.
  - But : en tapant une référence, voir tout de suite s'il y a un prix spécial (LPN) ou seulement le prix remisé.
  - À préciser : quel prix sert au calcul de la ligne (le plus bas des deux, comme aujourd'hui ?) et quelles colonnes vont sur le PDF.

- [x] Ligne « Titre » (en plus de « Texte »)
  - Nouveau bouton « Titre » à côté de « Texte ».
  - Mise en forme différente pour structurer le devis en paragraphes :
    - Titre : gras, rouge, taille 12 ;
    - Texte : noir, gras, taille 10–11.
  - Dans l'appli et sur le PDF.

- [x] Documentations produit jointes au PDF (fait : intégration de PDF Finder + « Devis PDF + Docs »)
  - Avant de générer le PDF, un bouton pour choisir sur l'ordinateur les documentations du matériel.
  - Elles sont ajoutées à la suite du devis : le PDF final fait plusieurs pages.

- [x] Réglages : rubrique « Interface »
  - Choix du thème : clair, sombre, ou automatique (suit le système).

- [x] Mises à jour simplifiées (testé sur PC le 2026-10-10 : mise à jour proposée et installée)
  - Voir si l'appli peut se mettre à jour toute seule depuis les releases GitHub
    (module de mise à jour de Tauri : vérification au démarrage, téléchargement, redémarrage).

- [x] Renommer l'application « Devis Groupe Cahors » (au lieu de « Devis Cahors »)

- [x] Numéro de devis de départ (Réglages → Config PDF → Devis → « Dernier n° déjà utilisé »)
  - Pouvoir indiquer dans les réglages un numéro minimum, pour que la numérotation de l'appli
    prenne la suite des devis déjà faits hors de l'appli (ex. si le dernier est 26-ABCD-0140,
    le prochain devis de l'appli est 26-ABCD-0141).

- [x] Détail d'une liste de prix dans le devis (tiroir latéral)
  - Clic sur le code d'une liste de prix (partie Client du devis) : ouvre un tiroir sur le côté
    avec tous les produits de la liste, et une recherche.
  - Liste forcée (favorites) : un bouton à côté du menu ouvre le même tiroir pour cette liste.

- [x] Listes Produits et Clients : 200 éléments par page par défaut

- [x] Listes de prix : clients rattachés dans un tiroir
  - Au-delà de 5 clients rattachés (la zone défile sans fin aujourd'hui) : bouton
    « Afficher les X clients » qui ouvre un tiroir latéral avec la liste des clients de la
    liste de prix, et de quoi en rattacher d'autres.

- [ ] Sécurité
  - [x] Désactiver les outils de développement (devtools) dans la version compilée
    (fait : absents par défaut, la CI échoue si la fonction « devtools » de Tauri est activée).
  - [x] Audit de sécurité de l'appli (2026-10-10).
  - [x] CSP (politique de sécurité du contenu) dans tauri.conf.json.
  - [x] Fichiers : boîtes de dialogue côté Rust, lecture / écriture / ouverture limitées aux
    chemins choisis par l'utilisateur ou enregistrés dans les réglages ; pas d'ATTACH / VACUUM
    sur l'index de la documentation.
  - [ ] CI : signer les mises à jour dans une étape à part (tags seulement), actions épinglées par
    SHA, `permissions: contents: read` par défaut, Node LTS.
  - [x] Points faibles : base restaurée vérifiée (integrity_check, pas de trigger ni de vue),
    gabarit Excel malformé sans plantage (testé sur des gabarits abîmés, filet de sécurité aussi
    sur l'import), taille maximale des fichiers lus (documents 300 Mo, import 200 Mo, gabarit
    20 Mo et 100 Mo par fichier décompressé, sauvegarde 1 Go).

- [x] Sauvegarde des données
  - [x] Sauvegarde (backup) de la base des devis (Réglages → Sauvegarde : copies datées,
    automatiques au démarrage, N gardées, restauration).
  - ~~Partage pair à pair (P2P) d'une liste de prix entre postes~~ : abandonné (2026-10-10),
    les listes viennent de la LPN nationale.

- [x] Listes de prix : éditeur et export
  - ~~Modifier une liste de prix dans l'appli (produits, prix)~~ : abandonné (2026-10-10),
    les listes viennent de la LPN nationale (un réimport écraserait les retouches).
  - [x] Export d'une liste de prix vers Excel (écran Clients, gabarit choisi dans les Réglages).

- [x] Écran « Statistiques » (par année : chiffres clés, devis par mois, meilleurs clients,
  commerciaux, produits les plus chiffrés)

- [x] Barre latérale : icône pour la replier / déployer comme sur claude.ai
  - Remplacer le chevron actuel ; deux états (ouverte / fermée) à voir ensemble.

- [x] Versions d'un devis
  - Pouvoir faire une nouvelle version d'un devis (à préciser ensemble le moment venu).
  - Fait : « Nouvelle version » (liste des devis et éditeur) : copie sous le numéro d'origine
    suffixé `-V2`, `-V3`… ; liens V1 / V2 / V3 dans l'éditeur ; anciennes versions estompées.

- [x] Produits : évolution du chiffrage d'une référence
  - Depuis la liste des produits, une option pour voir l'évolution du prix devisé de cette
    référence dans tous les devis (date, devis, client, quantité, prix unitaire…).
  - Filtre : restreindre à un ou plusieurs clients.
  - Écran à part (pas une fenêtre modale : pas assez lisible).

- [x] Suivi « Affaire obtenue »
  - Dans la liste des devis, une coche « Affaire obtenue » pour savoir si le devis s'est
    transformé en commande / facture (les factures ne sont pas gérées dans l'appli : simple suivi).
  - À exploiter ensuite dans les Statistiques (taux de transformation, montants obtenus).
  - Fait : coche et filtre dans la liste des devis ; Statistiques : affaires obtenues, taux,
    montant obtenu, part obtenue par mois, par client et par commercial (une version par devis).

- [x] Devis type (modèle)
  - Enregistrer un devis comme modèle (titres, lignes, quantités types) et créer un nouveau devis
    à partir d'un modèle, les prix étant recalculés pour le client choisi.
  - Fait : onglet « Devis types » (écran Devis), même éditeur que les devis sans client ni prix,
    « Enregistrer comme devis type » depuis un devis, « Partir d'un devis type » dans un nouveau devis.

- [x] Nettoyage des lignes d'un devis
  - Bouton « Supprimer les lignes sans quantité ».
  - Nettoyage intelligent si besoin : titres et sous-totaux devenus vides, lignes de texte vides,
    sous-totaux en double.
  - Fait : bouton « Nettoyer (n) » sous les lignes, avec confirmation détaillée.

- [x] Import LPN : clients de la feuille « CLIENTS IC » aussi importés (code et raison sociale seulement)
  - « gestion tarifs » prioritaire si le code est dans les deux feuilles.
  - Ces clients n'ont ni remise ni liste de prix : sur le devis, forcer une liste favorite.
- [x] Import LPN : un client créé par liste de prix rattachée à aucun client
  - Code = code de la liste sans le « T », nom = intitulé de la liste, liste rattachée.
  - Si ce code client existe déjà : la liste est rattachée à ce client.
- [x] Import LPN : code client en double dans « gestion tarifs » → première ligne conservée, avec les
  listes de prix de toutes les lignes.
- [x] Recherches : filtrage lancé après une courte pause de frappe (plus de ralentissement en tapant).

- [ ] Dépôt du code en privé (mises à jour servies par le service de licences)
  - [x] CI : versions officielles publiées aussi dans `devis-release` ; l'appli y cherche ses mises à jour.
  - [x] Créer `kappuccino/devis-release` (public, avec un README) et le secret `RELEASES_TOKEN`.
  - [x] Publier une version (0.1.11) : les postes l'installent depuis l'ancienne adresse et passent à la nouvelle.
  - [ ] Quand tous les postes sont en 0.1.11 ou plus : passer `kappuccino/devis` en privé.

- [ ] Service de licences et de mises à jour : `licence.cahors.kappuccino.net` (dossier `server/`, Node 26, JavaScript, Express + PocketBase)
  - [x] Installation en ligne de PocketBase et de Node (par Benjamin), accès transmis.
  - [x] PocketBase : tables utilisateurs (actif, nombre de postes) et postes (machine, dernière connexion, révoqué).
  - [x] Express : connexion, enregistrement du poste, ticket signé (clé privée sur le serveur seulement), renouvellement.
  - [x] Express : mises à jour servies par le service (`latest.json` et installeurs), réservées aux postes munis d'un ticket.
  - [x] Déploiement Coolify (Dockerfile prêt) : clé des tickets, variables, volume `/data`, liaison avec PocketBase.
  - [x] CI : déposer les versions sur le service (aussi dans `devis-release` pendant la transition).
  - [x] Secret GitHub `RELEASE_UPLOAD_TOKEN` (contenu de `~/release-token-devis.txt`).
  - [x] Appli : écran de connexion, vérification du ticket côté Rust, tolérance hors ligne, « Se déconnecter », page Aide.
  - [x] Appli : mises à jour cherchées sur le service, avec le ticket.
  - [ ] Transition : créer les comptes des postes existants avant de publier la version avec connexion.
  - [x] Publier la 0.2.0 (avec connexion) : dernière version aussi publiée dans `devis-release`.
  - [x] `devis-release` supprimé et retiré de la CI.
  - [ ] Passer `kappuccino/devis` en privé (tant qu'il est public, un poste resté en 0.1.x peut installer la 0.2.0 depuis ses releases).
  - [ ] Plus tard : sauvegarde en ligne chiffrée, diffusion du classeur des tarifs.

- [ ] Export / import d'un devis (fichier), pour partager un devis type entre collègues.

- [ ] (Idée, en attente) Import d'une étude Elium : le PDF (« Microsoft Print to PDF ») ne contient que
  des images. Pistes : vérifier d'abord si Elium exporte la liste du matériel (PDF texte, Excel) ; sinon
  LLM de vision via le service de licences (≈ 0,3 à 6 centimes par étude selon le modèle), avec
  vérification des références par le catalogue et validation à l'écran. Question de confidentialité
  (document envoyé à un tiers) à trancher.
