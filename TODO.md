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

- [ ] Mises à jour simplifiées
  - Voir si l'appli peut se mettre à jour toute seule depuis les releases GitHub
    (module de mise à jour de Tauri : vérification au démarrage, téléchargement, redémarrage).

- [x] Renommer l'application « Devis Groupe Cahors » (au lieu de « Devis Cahors »)
