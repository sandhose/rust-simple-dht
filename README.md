% SIMPLE_DHT(1)
% Quentin Gliech <quentin.gliech@etu.unistra.fr>
% Décembre 2017

# NOM

simple_dht – Implémentation d'une table de hashage distribuée en Rust

# SYNOPSIS

**simple_dht** [**-h**] \<commande>

# DESCRIPTION

**simple_dht** est une table de hashage distribuée simple écrite en Rust,
à base d'E/S asynchrones (via tokio-rs). Plusieurs serveurs peuvent s'échanger
des hash avec leur contenu. Un hash envoyé à un serveur sera propagé dans tout
le réseau.

Cet utilitaire comporte également un client, qui permet d'envoyer des messages
à un serveur.

Compilé et testé avec Rust 1.22.1 sur macOS.

# OPTIONS GÉNÉRALES

**-h**, **--help**
:   Affiche le message d'aide

# COMMANDES

**server [hote:port]**
:   Lance un serveur sur [hote:port] (par défaut: *[::]:0*)

**client \<hote:port> \<commande>**
:   Exécute une commande commandes sur un serveur distant

**help [sous-commande]**
:   Affiche l'aide d'une sous-commande

## SOUS-COMMANDE CLIENT

Ces commandes sont aussi utilisables dans le mode interactif du serveur.

**get \<hash>**
:   Récupère le contenu d'un hash

**put \<hash> \<contenu>**
:   Envoie un hash

**discover \<hote:port>**
:   Signale un nouveau pair au serveur distant

# FONCTIONNEMENT DU PROTOCOLE

Un serveur possède un état, comportant la liste des hash connus, la liste des
pairs connus et une liste de requêtes en attente. Lorsqu'il reçoit un message
quelconque d'une source, il ajoute cette source à la liste des pairs connus
(qu'elle soit un serveur ou un client). Toutes les secondes, le serveur envoie
un message `KeepAlive` à tous les pairs connus. Un pair n'ayant pas donné de
signe de vie depuis un certain temps est considéré comme dépassé, et est
supprimé de cette liste.

Lorsqu'un serveur reçois un message `Get(hash)`, il l'ajoute à la liste des
requêtes en attente. Il va regarder régulièrement si avec la liste des hash
qu'il connaît, il peut résoudre une des requêtes en attente. Si c'est le cas,
il envoie un message `Put(hash)` à celui qui a demandé le hash. 

Lorsqu'un serveur reçois un message `Put(hash, _)`, il ajoute le hash à sa
liste des hash connus, et envoie un message `IHave(hash)` à tous ses pairs
connus.

Lorsqu'un serveur reçois un message `IHave(hash)`, il vérifie s'il n'a pas déjà
le hash annoncé, et si ce n'est pas le cas, il le demande au pair distant en
envoyant un message `Get(hash)`.

Lorsqu'un serveur reçois un message `Discover(pair)`, il ajoute le pair à la
liste des pairs connus. À la boucle suivante, il enverra donc un `KeepAlive` au
pair tout juste découvert, et établira ainsi la connexion.

En pratique, l'état est une structure partagée par plusieurs agents: plusieurs
sockets UDP et l'invite de commande interactive. Rien n'empêche de faire
tourner ces agents dans des threads différents, ou d'implémenter relativement
rapidement le protocole par dessus une autre couche de transport (TCP).


# BUGS

Beaucoup d'erreurs ne sont pas attrapées proprement (mais il ne manque pas
grand chose pour qu'elles le soit). Un message invalide arrêtera le serveur,
tout comme la tentative d'envoi d'un paquet sur une destination non joignable.

L'invite de commande interactive manque de finition. Si un message apparaît
dans la console entre-temps, l'invite de commande ne s'affiche plus forcément
correctement.
