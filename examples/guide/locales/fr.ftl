# ANCHOR: catalog
resource-post = Article
    .plural = Articles
field-title = Titre
posts-published = { $count ->
    [one] { $count } article publié
   *[other] { $count } articles publiés
}
tablo-shell-sign-out = Quitter
# ANCHOR_END: catalog
