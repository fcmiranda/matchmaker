
nav mode
ctrl - mostra pastas pais
ctrl - a seleciona tudo
G - preview bottom


implementar 

### Proposta 1: Navegação Sem Fricção (*Seamless Traversal*) sem Alternância Modal
Para eliminar o problema de *keystroke slipping*, introduzir atalhos que funcionem **em ambos os modos (Input Focus e Results Focus)** sem exigir a tecla `Tab`:
- `Ctrl+l`: Entra imediatamente no diretório selecionado (`ChDir({=}) + Cancel query`).
- `Ctrl+h`: Sobe para o diretório pai (`ChDir(..) + Cancel query`).

### Proposta 2: Badges Semânticos de Escopo no Breadcrumb / Header
Quando o usuário ciclar fontes com `f` ou `ctrl-f` no `jump.toml`, o breadcrumb ou status deve exibir distintamente o modo de busca:

```
# Estado 1: Navegação Local
📁 LOCAL: /home/fecavmi/dev/github/matchmaker (12 pastas)

# Estado 2: Salto Global por Frecência (após apertar 'f')
⚡ FRECENCY (Top 50 Pastas Mais Acessadas)
```
- **Estilo Recomendado:** Badge em fundo invertido (`BgCyan + FgBlack` para `LOCAL`, `BgMagenta + FgWhite` para `FRECENCY`).




improve separators site


whats the keybinding to scrollback tmux?
- i need to find some word on the currenty chat
prefix (hold) and e
- see some way to exit fast


breadcrumb pulando a linha do prompt cmd
comando para busca global
