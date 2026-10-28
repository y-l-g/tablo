# Le français de Tablo : chaque message de `en.ftl`, avec ses variables. Le catalogue d'une
# application remplace n'importe lequel par son identifiant (`Panel::translations`).

## Shell

tablo-shell-navigation = Navigation
tablo-shell-close-sidebar = Fermer le menu latéral
tablo-shell-home = Accueil
tablo-shell-account = Compte
tablo-shell-profile = Profil
tablo-shell-two-factor = Connexion à deux facteurs
tablo-shell-sign-out = Se déconnecter
tablo-shell-theme = Thème
tablo-shell-theme-light = Clair
tablo-shell-theme-dark = Sombre
tablo-shell-theme-system = Système
tablo-shell-language = Langue
    .en = Anglais
    .fr = Français
tablo-shell-tenants = Espaces
tablo-shell-switch-tenant = Changer d'espace
tablo-shell-select-tenant = Choisir un espace
tablo-shell-tenant-settings = Paramètres de l'espace
tablo-shell-create-tenant = Créer un espace
tablo-crumb-create = Créer
tablo-crumb-view = Voir
tablo-crumb-edit = Modifier

## Error pages

tablo-error-not-found = Page introuvable
tablo-error-not-found-description = La page demandée n'existe pas, ou elle a été déplacée.
tablo-error-forbidden = Accès refusé
tablo-error-forbidden-description = Vous n'avez pas l'autorisation d'ouvrir cette page.
tablo-error-failed = Une erreur est survenue
tablo-error-failed-description = La page n'a pas pu être affichée. Réessayez dans un instant.
tablo-error-home = Retour à l'accueil

## Global search

tablo-search = Rechercher
tablo-search-placeholder = Rechercher…
tablo-search-no-results = Aucun résultat
tablo-search-all = Voir tous les résultats
tablo-search-nothing-matches = Rien ne correspond à « { $term } ».
tablo-search-detail = { $label } :

## Common

tablo-cancel = Annuler
tablo-confirm = Confirmer
tablo-action-view = Voir
tablo-action-edit = Modifier
tablo-action-delete = Supprimer
tablo-yes = Oui
tablo-no = Non

## Lists

tablo-list-search = Rechercher dans le tableau
tablo-list-columns = Colonnes
tablo-list-columns-reset = Réinitialiser
tablo-list-columns-apply = Appliquer
tablo-list-delete-selected = Supprimer
tablo-list-select-row = Sélectionner la ligne
tablo-list-actions = Actions
tablo-confirm-run = Lancer cette action ?
tablo-confirm-delete-record = Supprimer cet enregistrement ?
tablo-confirm-delete-selected = Supprimer les enregistrements sélectionnés ?
tablo-confirm-irreversible = Cette action est irréversible.
tablo-confirm-selected = Enregistrements sélectionnés :
tablo-list-select-all = Sélectionner toutes les lignes
tablo-list-sort = Trier par { $column } { $direction ->
    [descending] décroissant
   *[ascending] croissant
}
tablo-list-empty = Aucun enregistrement pour l'instant
tablo-list-empty-search = Aucun résultat pour « { $term } »
tablo-list-empty-filters = Aucun résultat pour ces filtres
tablo-list-clear-search = Effacer la recherche
tablo-list-clear-filters = Effacer les filtres
tablo-list-filters = Filtres
tablo-list-reset-filters = Réinitialiser
tablo-list-first-page = Revenir à la première page
tablo-pager-showing = { $first }–{ $last } sur { $total }
tablo-pager-page = Page { $page }
tablo-pager-previous = Précédent
tablo-pager-next = Suivant
tablo-pager-per-page = Par page
tablo-pager-apply = Appliquer
tablo-list-select-first = Sélectionnez d'abord au moins une ligne
tablo-list-failed = Impossible de charger { $label }
tablo-list-failed-description = Une erreur est survenue pendant le chargement des enregistrements.
tablo-list-retry = Réessayer
tablo-list-export = Exporter en CSV
tablo-list-import = Importer un CSV

tablo-column-date = { $day } { $month ->
        [1] janv.
        [2] févr.
        [3] mars
        [4] avr.
        [5] mai
        [6] juin
        [7] juil.
        [8] août
        [9] sept.
        [10] oct.
        [11] nov.
       *[12] déc.
    } { $year }

## Filters

tablo-filter-ignored = Filtre(s) ignoré(s) :
tablo-filter-ignored-entry = { $filter } ({ $reason })
tablo-filter-ignored-consequence = { $unfiltered ->
    [true] — résultats non filtrés.
   *[false] — les autres filtres s'appliquent.
}
tablo-filter-ignored-unknown = filtre inconnu
tablo-filter-ignored-dropped = plus de { $filters } filtres, plus de { $values } valeurs pour l'un d'eux, plus de { $bytes } octets, ou l'ancienne forme filters=
tablo-filter-ignored-invalid = valeur invalide
tablo-filter-all = Tous
tablo-filter-true = Vrai
tablo-filter-false = Faux
tablo-trash = Corbeille
tablo-trash-without = Sans la corbeille
tablo-trash-with = Avec la corbeille
tablo-trash-only = Corbeille seulement

## Actions

tablo-action-restore = Restaurer
tablo-action-force-delete = Supprimer définitivement
tablo-action-deleted = { $count ->
    [one] Supprimé
   *[other] { $count } enregistrements supprimés
}
tablo-action-restored = { $count ->
    [one] Restauré
   *[other] { $count } enregistrements restaurés
}
tablo-action-force-deleted = { $count ->
    [one] Supprimé définitivement
   *[other] { $count } enregistrements supprimés définitivement
}
tablo-confirm-restore = Restaurer cet enregistrement ?
tablo-confirm-force-delete = Supprimer définitivement cet enregistrement ?
tablo-action-skipped = ({ $skipped } sur { $selected } ignorés)
tablo-action-on-record = { $action } : { $record }
tablo-action-on-records = { $action } : { $count ->
    [one] { $count } enregistrement
   *[other] { $count } enregistrements
}
tablo-action-done = { $action } : terminé
tablo-action-refused-all = { $action } : { $count ->
    [one] { $count } enregistrement sélectionné ne peut pas recevoir cette action ; rien n'a été modifié
   *[other] { $count } enregistrements sélectionnés ne peuvent pas recevoir cette action ; rien n'a été modifié
}

## Write failures

tablo-failed-description = Rien n'a été modifié — réessayez.
tablo-failed-create = Impossible de créer l'enregistrement
tablo-failed-update = Impossible d'enregistrer les modifications
tablo-failed-import = Impossible d'importer le fichier
tablo-failed-delete = Impossible de supprimer l'enregistrement
tablo-failed-bulk-delete = Impossible de supprimer les lignes sélectionnées
tablo-failed-restore = Impossible de restaurer les enregistrements
tablo-failed-force-delete = Impossible de supprimer définitivement les enregistrements
tablo-failed-run = Impossible de lancer l'action
tablo-failed-attach = Impossible d'associer l'enregistrement
tablo-failed-detach = Impossible de dissocier les enregistrements

## Forms

tablo-form-created = Créé
tablo-form-updated = Mis à jour
tablo-form-create-title = Créer { $label }
tablo-form-edit-title = Modifier { $label }
tablo-form-create = Créer
tablo-form-save = Enregistrer
tablo-form-stale = Cet enregistrement a changé depuis que vous l'avez ouvert
tablo-form-stale-description = Enregistrer à nouveau remplace ces modifications par les vôtres. Rechargez la page pour les voir d'abord.
tablo-form-unseen = Un champ que vous ne voyez pas refuse cet enregistrement
tablo-form-unseen-description = Demandez à quelqu'un qui peut le modifier de corriger l'enregistrement.

## Relations

tablo-relation-attach = Associer
tablo-relation-attached = Associé
tablo-relation-detached = { $count ->
    [one] { $count } enregistrement dissocié
   *[other] { $count } enregistrements dissociés
}
tablo-relation-attach-title = Associer { $label }
tablo-relation-detach = Dissocier
tablo-relation-new = Créer { $label }

## Import

tablo-import-imported = { $count ->
    [one] { $count } ligne importée
   *[other] { $count } lignes importées
}
tablo-import-title = Importer { $label }
tablo-import-description = Chaque ligne du fichier crée un enregistrement dans « { $label } ». La première ligne nomme les colonnes ; une ligne refusée n'importe rien, corrigez donc le fichier et importez-le à nouveau.
tablo-import-template = Télécharger le modèle
tablo-import-file = Fichier CSV
tablo-import-submit = Importer
tablo-import-row = Ligne
tablo-import-column = Colonne
tablo-import-field = Champ
tablo-import-problem = Problème
tablo-import-required = Obligatoire
tablo-import-repeat = Répétez la colonne pour chaque valeur
tablo-import-refused = { $count ->
    [one] { $count } valeur refusée : rien n'a été importé
   *[other] { $count } valeurs refusées : rien n'a été importé
}
tablo-import-no-file = Choisissez un fichier CSV à importer
tablo-import-not-utf8 = Le fichier n'est pas du texte UTF-8 : enregistrez-le en CSV UTF-8
tablo-import-unterminated = Une cellule entre guillemets ouverte ligne { $line } n'est jamais fermée
tablo-import-no-header = Le fichier est vide : sa première ligne nomme les colonnes
tablo-import-no-rows = Le fichier contient un en-tête mais aucune ligne
tablo-import-too-many-rows = Un fichier contient au plus { $max } lignes
tablo-import-too-many-cells = La ligne { $row } contient plus de { $max } cellules
tablo-import-unknown-columns = { $count ->
    [one] Aucun champ ne correspond à la colonne { $columns }
   *[other] Aucun champ ne correspond aux colonnes { $columns }
}
tablo-import-ambiguous-columns = Plusieurs champs s'intitulent { $columns } : nommez la colonne par sa clé
tablo-import-repeated-columns = L'en-tête nomme { $columns } plus d'une fois
tablo-import-stray-value = La ligne contient une valeur hors de toute colonne de l'en-tête
tablo-import-unseen = Un champ que vous ne voyez pas refuse cette ligne

## Detail pages

tablo-detail-back = Retour à la liste
tablo-record-title = { $label } { $key }

## Fields

tablo-field-required = { $label } est obligatoire
tablo-field-unreadable = La valeur n'a pas pu être lue
tablo-field-unavailable = { $label } n'est pas disponible
tablo-field-select = -- Choisir --
tablo-repeater-add = Ajouter un élément
tablo-field-invalid = { $label } est invalide
tablo-field-too-many-choices = { $label } accepte au plus { $max } nouveaux choix
tablo-field-options-retry = { $label } n'a pas pu charger ses options, réessayez
tablo-field-options-failed = { $label } n'a pas pu charger ses options
tablo-field-filter = Filtrer…
tablo-field-filter-options = Filtrer les options de { $label }
tablo-field-no-options = Aucune option
tablo-field-no-matching-options = Aucune option correspondante
tablo-field-searching = Recherche…
tablo-field-too-many-options = Trop d'options — tapez pour chercher
tablo-field-accepted = { $label } doit être accepté
tablo-field-email = { $label } doit être une adresse e-mail valide
tablo-field-url = { $label } doit être une URL valide commençant par http:// ou https://
tablo-field-min-length = { $label } doit contenir au moins { $min } caractères
tablo-field-max-length = { $label } doit contenir au plus { $max } caractères
tablo-field-min = { $label } doit valoir au moins { $min }
tablo-field-max = { $label } doit valoir au plus { $max }
tablo-field-pattern = { $label } n'a pas le format attendu
tablo-field-min-options = { $count ->
    [one] { $label } demande au moins { $count } option
   *[other] { $label } demande au moins { $count } options
}
tablo-field-max-options = { $count ->
    [one] { $label } accepte au plus { $count } option
   *[other] { $label } accepte au plus { $count } options
}
tablo-field-min-items = { $count ->
    [one] { $label } demande au moins { $count } élément
   *[other] { $label } demande au moins { $count } éléments
}
tablo-field-max-items = { $count ->
    [one] { $label } accepte au plus { $count } élément
   *[other] { $label } accepte au plus { $count } éléments
}
tablo-field-different = { $label } doit être différent de { $other }
tablo-field-gt = { $label } doit être supérieur à { $other }
tablo-field-gte = { $label } doit valoir au moins { $other }
tablo-field-lt = { $label } doit être inférieur à { $other }
tablo-field-lte = { $label } doit valoir au plus { $other }
tablo-field-after = { $label } doit être postérieur à { $other }
tablo-field-not-before = { $label } ne doit pas être antérieur à { $other }
tablo-field-before = { $label } doit être antérieur à { $other }
tablo-field-not-after = { $label } ne doit pas être postérieur à { $other }
tablo-field-extension = { $count ->
    [one] { $label } doit être un fichier se terminant par { $extensions }
   *[other] { $label } doit être un fichier se terminant par l'une de ces extensions : { $extensions }
}
tablo-field-max-size = { $label } doit peser au plus { $max }
tablo-field-upload-failed = { $label } n'a pas pu être envoyé : { $reason }
tablo-size-bytes = { $count ->
    [one] { $count } octet
   *[other] { $count } octets
}
tablo-size-kib = { $count } Kio
tablo-size-mib = { $count } Mio
tablo-rich-toolbar = Mise en forme
tablo-rich-bold = Gras
tablo-rich-italic = Italique
tablo-rich-underline = Souligné
tablo-rich-strike = Barré
tablo-rich-code = Code
tablo-rich-link = Lien
tablo-rich-link-url = URL du lien
tablo-rich-heading-1 = Titre 1
tablo-rich-heading-2 = Titre 2
tablo-rich-heading-3 = Titre 3
tablo-rich-bullet-list = Liste à puces
tablo-rich-ordered-list = Liste numérotée
tablo-rich-blockquote = Citation
tablo-rich-code-block = Bloc de code
tablo-rich-horizontal-rule = Ligne horizontale
tablo-rich-undo = Annuler
tablo-rich-redo = Rétablir
# `kind` is a `tablo-type-*` message, which carries its article.
tablo-field-not-a = `{ $value }` n'est pas { $kind } valide
tablo-field-not-an-option = `{ $value }` n'est pas une option valide
tablo-type-integer = un nombre entier
tablo-type-boolean = une valeur oui/non
tablo-type-number = un nombre
tablo-type-identifier = un identifiant
tablo-type-timestamp = un horodatage
tablo-type-date = une date
tablo-type-time = une heure
tablo-type-date-time = une date et heure
tablo-file-keep = Laissez vide pour garder le fichier actuel.
tablo-file-remove = Retirer le fichier actuel
tablo-file-current = Actuel :
tablo-repeater-up = Monter
tablo-repeater-down = Descendre
tablo-repeater-remove = Retirer
tablo-field-taken = { $label } est déjà utilisé
tablo-field-too-many-results = Trop de résultats — continuez à taper

## Widgets

tablo-stat-trend-up = En hausse
tablo-stat-trend-down = En baisse
tablo-stat-trend-flat = Stable
tablo-chart-empty = Aucune donnée
tablo-record-table-view-all = Tout voir

## Notifications

tablo-notifications = Notifications
tablo-notifications-close = Fermer les notifications
tablo-toast-close = Fermer
tablo-notifications-none = Aucune notification
tablo-notifications-caught-up = Vous êtes à jour.
tablo-notifications-unread = { $count ->
    [0] Tout est lu
    [one] { $count } non lue
   *[other] { $count } non lues
}
tablo-notifications-read-all = Tout marquer comme lu
tablo-notifications-clear = Effacer
tablo-notifications-more = Afficher plus
tablo-notification-read = Marquer comme lue
tablo-notification-unread = Marquer comme non lue
tablo-notification-remove = Retirer
tablo-ago-now = à l'instant
tablo-ago-minutes = il y a { $count } min
tablo-ago-hours = il y a { $count } h
tablo-ago-days = il y a { $count } j

## Sign-in

tablo-auth-name = Nom
tablo-auth-email = E-mail
tablo-auth-password = Mot de passe
tablo-auth-confirm-password = Confirmer le mot de passe
tablo-auth-current-password = Mot de passe actuel
tablo-auth-new-password = Nouveau mot de passe
tablo-auth-confirm-new-password = Confirmer le nouveau mot de passe
tablo-login-title = Connexion
tablo-login-description = Bon retour. Connectez-vous à { $brand }.
tablo-login-identifier = E-mail ou nom d'utilisateur
tablo-login-forgot = Mot de passe oublié ?
tablo-login-submit = Se connecter
tablo-login-no-account = Pas encore de compte ?
tablo-login-sign-up = S'inscrire
tablo-login-failed = E-mail ou mot de passe invalide.
tablo-login-unavailable = La connexion est indisponible pour le moment. Réessayez dans un instant.
tablo-gate-description = Votre compte ne peut pas ouvrir { $brand }.

## Sign-up

tablo-sign-up-title = Créer un compte
tablo-sign-up-description = Inscrivez-vous à { $brand }.
tablo-sign-up-submit = S'inscrire
tablo-sign-up-have-account = Déjà un compte ?
tablo-sign-up-sign-in-link = Se connecter
tablo-sign-up-created = Compte créé
tablo-sign-up-pending = Vous pourrez vous connecter une fois le compte approuvé.
tablo-sign-up-sign-in = Connectez-vous pour continuer.
tablo-sign-up-throttled = Trop de tentatives d'inscription. Réessayez dans un instant.
tablo-sign-up-unavailable = L'inscription est indisponible pour le moment. Réessayez dans un instant.
tablo-password-show = Afficher le mot de passe
tablo-password-hide = Masquer le mot de passe
tablo-password-too-short = Le mot de passe doit contenir au moins { $min } caractères
tablo-password-mismatch = Les mots de passe ne correspondent pas

## Password reset

tablo-reset-forgot-title = Mot de passe oublié ?
tablo-reset-forgot-description = Saisissez votre e-mail et nous vous enverrons un lien pour choisir un nouveau mot de passe.
tablo-reset-request = M'envoyer un lien de réinitialisation
tablo-reset-enter-email = Saisissez votre adresse e-mail
tablo-reset-throttled = Trop de demandes de réinitialisation. Réessayez dans un instant.
tablo-reset-unavailable = La réinitialisation du mot de passe est indisponible pour le moment. Réessayez dans un instant.
tablo-reset-sent = Consultez vos e-mails
tablo-reset-sent-description = Si un compte utilise cette adresse, il a reçu un lien pour réinitialiser son mot de passe.
tablo-reset-title = Choisir un nouveau mot de passe
tablo-reset-description = Utilisez au moins { $min } caractères.
tablo-reset-submit = Réinitialiser le mot de passe
tablo-reset-done = Mot de passe réinitialisé
tablo-reset-done-description = Connectez-vous avec votre nouveau mot de passe.
tablo-reset-invalid = Ce lien de réinitialisation est invalide ou a expiré.
tablo-reset-new-link = Demander un nouveau lien
tablo-reset-expired-title = Lien de réinitialisation expiré
tablo-reset-expired-description = Chaque lien fonctionne une fois, pendant une heure.
tablo-reset-remember = Vous vous souvenez de votre mot de passe ?
tablo-reset-mail-subject = Réinitialisez votre mot de passe { $brand }
tablo-reset-mail-body =
    Quelqu'un a demandé à réinitialiser le mot de passe de votre compte { $brand }.

    Ouvrez ce lien pour en choisir un nouveau. Il fonctionne une fois, pendant les { $minutes } prochaines minutes :

    { $link }

    Si vous n'avez rien demandé, ignorez cet e-mail : votre mot de passe reste le même.

## Email verification

tablo-verify-title = Vérifiez votre e-mail
tablo-verify-description = Ouvrez le lien que nous envoyons à { $email } pour confirmer qu'il s'agit bien de vous, puis revenez.
tablo-verify-request = M'envoyer un lien de vérification
tablo-verify-sent-description = Un nouveau lien de vérification est en route.
tablo-verify-done = E-mail vérifié
tablo-verify-invalid = Ce lien de vérification est invalide ou a expiré.
tablo-verify-new-link = Connectez-vous pour recevoir un nouveau lien
tablo-verify-expired-title = Lien de vérification expiré
tablo-verify-expired-description = Chaque lien fonctionne une fois, pendant un jour, pour l'adresse à laquelle il a été envoyé.
tablo-verify-mail-subject = Vérifiez votre adresse e-mail { $brand }
tablo-verify-mail-body =
    Ouvrez ce lien pour confirmer que { $email } est l'adresse de votre compte { $brand }. Il fonctionne une fois, pendant les 24 prochaines heures :

    { $link }

    Si vous n'avez pas créé ce compte, ignorez cet e-mail.

## Email change

tablo-email-change-confirm-title = Confirmez votre nouvel e-mail
tablo-email-change-confirm-description = Faire de { $email } l'adresse avec laquelle vous vous connectez.
tablo-email-change-done = E-mail modifié
tablo-email-change-done-description = Vous vous connectez désormais avec { $email }.
tablo-email-change-keep-title = Garder votre e-mail
tablo-email-change-keep-description = Garder { $email } comme adresse de connexion, même si le changement a eu lieu, et déconnecter tous les navigateurs.
tablo-email-change-keep = Garder mon adresse
tablo-email-change-kept = E-mail conservé
tablo-email-change-kept-description = Vous vous connectez avec { $email }, et tous les navigateurs sont déconnectés. Réinitialisez votre mot de passe si quelqu'un d'autre le connaît.
tablo-email-change-mail-subject = Confirmez votre nouvelle adresse e-mail { $brand }
tablo-email-change-mail-body =
    Ouvrez ce lien pour faire de { $new } l'adresse de votre compte { $brand }. Il fonctionne une fois, pendant les 24 prochaines heures :

    { $link }

    Si vous n'avez pas demandé ce changement, ignorez cet e-mail.
tablo-email-change-notice-subject = L'adresse e-mail de votre compte { $brand } va changer
tablo-email-change-notice-body =
    Quelqu'un a demandé à remplacer l'adresse de votre compte { $brand } par { $new }.

    Si ce n'était pas vous, ouvrez ce lien dans les 7 jours pour garder { $old }, même si le changement a eu lieu, et déconnecter tous les navigateurs, puis réinitialisez votre mot de passe :

    { $link }

## Profile

tablo-profile-saved = Profil enregistré
tablo-profile-email-pending = Ouvrez le lien envoyé à votre nouvelle adresse pour commencer à l'utiliser.
tablo-profile-current-password-needed = Saisissez votre mot de passe actuel pour changer votre e-mail ou votre mot de passe
tablo-profile-current-password-wrong = Le mot de passe actuel est incorrect
tablo-profile-password-help = Laissez vide pour garder le mot de passe actuel.
tablo-profile-current-password-help = Obligatoire pour changer votre e-mail ou votre mot de passe.

## Tenants

tablo-tenant-saved = Espace enregistré
tablo-tenant-not-saved = Rien n'a été enregistré
tablo-tenant-switched = Vous agissez désormais pour { $tenant }. Vérifiez ses paramètres avant d'enregistrer.
tablo-tenant-created = Espace créé
tablo-tenant-acting = Vous agissez désormais pour { $tenant }.
tablo-tenant-create-title = Créer un espace
tablo-tenant-create-first = Créez l'espace dans lequel vous travaillez sur { $brand }.
tablo-tenant-create-another = Créez un autre espace dans lequel travailler sur { $brand }.
tablo-tenant-changed-mind = Vous avez changé d'avis ?
tablo-tenant-go-back = Revenir

## Two-factor sign-in

tablo-two-factor-app-description = Saisissez le code affiché par votre application d'authentification.
tablo-two-factor-mailed-description = Saisissez le code que nous vous avons envoyé.
tablo-two-factor-code-label = Code d'authentification
tablo-two-factor-code-help = Ou l'un de vos codes de récupération.
tablo-two-factor-verify = Vérifier
tablo-two-factor-resend = Envoyer un nouveau code
tablo-two-factor-sent = Un nouveau code est en route.
tablo-two-factor-invalid = Ce code n'est pas valide. Essayez le code actuel.
tablo-two-factor-not-you = Ce n'est pas vous ?
tablo-two-factor-switch = Se connecter avec un autre compte
tablo-two-factor-expired = Reconnectez-vous
tablo-two-factor-expired-description = La connexion a attendu son code trop longtemps.
tablo-two-factor-ended-description = Trop de codes erronés. Saisissez votre mot de passe pour réessayer.
tablo-two-factor-alert-subject = Codes de connexion erronés sur votre compte { $brand }
tablo-two-factor-alert-body =
    Votre mot de passe a servi à vous connecter à { $brand }, puis plusieurs codes à deux facteurs erronés l'ont suivi.

    Si ce n'était pas vous, changez votre mot de passe : quelqu'un d'autre le connaît.
tablo-two-factor-off = La connexion à deux facteurs est désactivée. Activez-la pour qu'un code vous soit demandé à chaque connexion.
tablo-two-factor-password-help = Confirmez votre identité avant d'ajouter un second facteur.
tablo-two-factor-wrong-password = Ce mot de passe n'est pas le bon.
tablo-two-factor-start-app = Configurer une application d'authentification
tablo-two-factor-start-email = Recevoir les codes par e-mail
tablo-two-factor-email-setup = Saisissez le code envoyé à { $address } pour activer les codes par e-mail.
tablo-two-factor-code = Code
tablo-two-factor-email-help = Les six chiffres de l'e-mail.
tablo-two-factor-app-help = Les six chiffres affichés par l'application.
tablo-two-factor-scan = Scannez le code avec une application d'authentification, ou saisissez la clé à la main, puis saisissez le code qu'elle affiche.
tablo-two-factor-qr = QR code pour une application d'authentification
tablo-two-factor-turn-on = Activer
tablo-two-factor-turn-off = Désactiver
tablo-two-factor-on = La connexion à deux facteurs est activée
tablo-two-factor-on-app = La connexion à deux facteurs est activée, avec une application d'authentification. { $left ->
    [one] Il reste { $left } code de récupération.
   *[other] Il reste { $left } codes de récupération.
}
tablo-two-factor-on-email = La connexion à deux facteurs est activée, avec des codes envoyés à { $address }. { $left ->
    [one] Il reste { $left } code de récupération.
   *[other] Il reste { $left } codes de récupération.
}
tablo-two-factor-on-app-help = Un code de l'application, ou un code de récupération.
tablo-two-factor-on-email-help = Un code reçu par e-mail, ou un code de récupération.
tablo-two-factor-new-codes = Afficher de nouveaux codes de récupération
tablo-two-factor-keep-codes = Conservez ces codes de récupération en lieu sûr. Chacun vous connecte une fois sans l'application, et ils ne seront plus affichés.
tablo-two-factor-done = Terminé
tablo-two-factor-sent-toast = Code envoyé
tablo-two-factor-off-toast = Connexion à deux facteurs désactivée
tablo-two-factor-mail-subject = Votre code de connexion { $brand }
tablo-two-factor-mail-body =
    Votre code pour vous connecter à { $brand } est { $code }. Il fonctionne une fois, pendant les 10 prochaines minutes.

    Si vous n'avez pas essayé de vous connecter, changez votre mot de passe : quelqu'un d'autre le connaît.
