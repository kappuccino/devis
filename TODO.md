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

- [x] Nouvelle numérotation des devis : `26-JMOS-0001`, `26-JMOS-0002`…
  - Année sur 2 chiffres, puis préfixe (Réglages), puis compteur sur 4 chiffres.
  - Aujourd'hui : `JMOS-2026-0001`. Décider si les devis existants sont renumérotés ou gardent leur numéro.

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
    prenne la suite des devis déjà faits hors de l'appli (ex. si le dernier est 26-JMOS-0140,
    le prochain devis de l'appli est 26-JMOS-0141).

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
  - [ ] Points faibles : base restaurée vérifiée (integrity_check, pas de trigger), gabarit Excel
    malformé sans plantage, taille maximale des fichiers lus.

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
