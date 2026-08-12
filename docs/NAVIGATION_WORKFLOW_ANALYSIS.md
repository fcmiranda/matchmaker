# Análise de Alto Desempenho & TUI UX: Matchmaker (`--nav` & `nav_bar`) vs Ecossistema CLI

> **Autor:** Especialista em Sistemas de Alto Desempenho, Rust Async & Terminal User Interfaces (Ratatui, Crossterm, Kitty/Sixel).
> **Data:** Agosto de 2026
> **Escopo:** Análise arquitetural e de UX do repositório `matchmaker` (`matchmaker-cli`, `matchmaker-lib`, `matchmaker-partial`, `matchmaker-partial-macros`) com foco no modo de navegação modal (`--nav`), indicador visual (`nav_bar`), e comparação comparativa com ferramentas modernas de navegação rápida de diretórios/arquivos.

---

## 1. Contexto Arquitetural do Repositório Matchmaker

O `matchmaker` é um monorepo Rust composto por quatro crates principais:
- **`matchmaker-cli`**: Parseamento de argumentos (Clap), overrides de CLI, sistema de presets (`jump.toml`, `rg.toml`, `git`), overlays de gerenciamento de arquivos (`fm.rs`) e handlers de ações de terminal.
- **`matchmaker-lib`**: Motor central do picker, engine de correspondência fuzzy `nucleo` (com suporte a frecência, penalidade de profundidade e ordenação de 3 níveis `dir_first`), loop de eventos, renderizador Ratatui/Crossterm e sistema de *preview* concorrente (`Previewer` + `ratatui_image`).
- **`matchmaker-partial` & `matchmaker-partial-macros`**: Sistema de mesclagem parcial de configurações (*partial-struct merge*) que permite combinar configurações TOML, presets e overrides da linha de comando de forma declarativa e fortemente tipada.

### O Workflow Modal (`--nav`) e a `nav_bar`

No `matchmaker`, a flag `--nav` (antigo `--ui-fm`) transforma o filtro fuzzy tradicional em uma **interface de navegação modal de zero fricção**:
- **Divisão Modal (Modal Split)**: A tecla `Tab` alterna a entrada do teclado entre a barra de busca/filtro (`Input Focus`) e a lista de resultados (`Results Focus`).
- **Indicador Visual (`nav_bar`)**: Na borda esquerda da lista de resultados, a `nav_bar` fornece um indicador visual customizável (`Thick`, `Rounded`, `Double`, `QuadrantOutside`) com controle de brilho/piscagem (`blink_rate`), negrito (`bold`), marcação ativa (`>`) e alteração no prompt do filtro (`[NAV]`).
- **Overlays Integrados (`fm.rs`)**: Operações rápidas de arquivos ativadas via navegação direta por teclas de caractere único:
  - `a`: Criar arquivo/diretório
  - `r`: Renomear
  - `d`: Deletar (com envio para a Lixeira do sistema e pilha de Undo/Redo)
  - `y` / `x` / `p`: Copiar / Recortar / Colar
  - `z` / `Z`: Compactar / Descompactar arquivos (Zip, Tar.gz, Tar.bz2, Tar.xz)
  - `l` / `h`: Entrar no diretório selecionado (`ChDir("{=}") + Reload`) / Subir diretório (`ChDir("..") + Reload`)

---

## 2. Matriz Comparativa: Matchmaker vs Ferramentas Modernas de Navegação

| Característica | **Matchmaker (`--nav`)** | **Yazi** | **Superfiles** | **fff** | **fzf / Skim** | **Broot** |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Linguagem / Runtime** | Rust (Ratatui/Tokio) | Rust (Ratatui/Tokio) | Go (Bubbletea) | Bash Shell | Go / Rust | Rust |
| **Arquitetura de Layout** | 2 Panes (Picker + Preview) + Overlay Modal | 3 Colunas (Pai - Atual - Preview) | Multi-painel responsivo | Painel único minimalista | 1 Coluna + Preview opcional | Visão em Árvore (Tree) |
| **Motor de Busca / Filtering** | Multi-threaded **Nucleo** (Frecência + Depth Penalty) | Busca em Regex / Glob + Ripgrep | Filtro básico de strings | N/A (Manual nav) | N/A (Fzf engine) | Fzf-like tree filter |
| **Navegação de Zero Fricção** | Modal Split (`Tab` switch), `h`/`l` traversal, Presets (`jump.toml`) | VIM keys (`h`/`j`/`k`/`l`), Zoxide integrado | Teclas de seta + VIM keys | Teclas VIM puras | Pipelined query search | Direct tree typing |
| **Protocolos Gráficos (Imagens)** | Kitty, Sixel, Iterm2, Halfblocks (`ratatui_image`) | Kitty, Sixel, Uberzugpp, Chafa | Limites de caracteres ASII/Halfblocks | Nenhum | Kitty/Sixel (via ueberzug/chafa previewer) | Limites de texto |
| **Desempenho / Latência** | Microsegundos (Nucleo Crate) | Baixíssima (Async I/O Lua event loop) | Média (Go GC runtime overhead) | Depende do Shell execution | Extremamente rápido | Rápido |
| **Extensibilidade** | Dynamic Handlers + Proc Macro Configs + Presets | Plugins em Lua + Async jobs | Configuração YAML simples | Shell scripts | Shell functions & `--bind` | Custom verbs / commands |
| **Saída / Shell `cd` Integration** | Suporta via `Become` / `ChDir` & Presets | Integrado via wrapper script | Integrado | Nativo (`cd` ao sair) | Requer wrapper shell (`fe`, `z`) | Nativo `br` function |

---

## 3. Análise Detalhada de UX & Desempenho: O que pode ser melhorado?

### A. Melhorias de UX & Zero-Friction (Navegação Terminal)

1. **Visão Tripla Contextual (Parent - Current - Child Preview) [CONCLUÍDO]**:
   - *Status*: Implementado (`--parent-peek`). Painel lateral esquerdo minimizado exibindo o conteúdo do diretório pai com a pasta atual destacada e auto-centralizada.

2. **Integração Nativa de Transição de Diretório ao Sair (`cd` on Exit)**:
   - *Situação Atual*: A troca de diretório ocorre via `ChDir` dentro do processo do `matchmaker`. Ao sair, o shell pai permanece no diretório original a menos que o output seja capturado por um alias externo.
   - *Oportunidade*: Disponibilizar um comando/wrapper oficial (`mmcd` ou `--cwd-file /tmp/mm-cwd`) gerado via CLI, permitindo que a navegação no TUI persista instantaneamente no shell ao encerrar com `Enter` ou tecla dedicada.

3. **Inclusão Nativa de Frecência / Zoxide no Nível 0 do Nucleo**:
   - *Situação Atual*: A navegação rápida entre diretórios frequentes é alcançada via preset (`jump.toml`).
   - *Oportunidade*: Incorporar a pontuação de frecência diretamente no ordenador de 3 camadas (`dir_first` Tier 0) no motor Nucleo quando em modo `--nav`, permitindo saltar para pastas mais acessadas sem precisar acionar um preset separado.

4. **Feedback Visual Aprimorado na `nav_bar`**:
   - *Situação Atual*: A `nav_bar` alterna entre borda ativa e estado de piscagem (`blink_rate`).
   - *Oportunidade*:
     - Adicionar transição sutil de cor no fundo do item selecionado ao alternar entre `Input Focus` e `Results Focus` (ex: azul esmaecido em Input vs cyan vibrante em Results).
     - Exibir atalhos de ações disponíveis no footer dinamicamente quando a `nav_bar` estiver focada (ex: `[a] Add  [r] Rename  [d] Trash  [y] Yank  [p] Paste`).

5. **Iconografia Avançada e Colorização por Extensão (Estilo `eza` / `lsd`)**:
   - *Situação Atual*: Suporte a ícones em linha e formatação customizada via tabelas Ratatui.
   - *Oportunidade*: Mapeamento direto de ícones Nerdfonts integrados com suporte a esquema de cores respeitando `LS_COLORS` para tipos de arquivos (executáveis, mídias, código fonte, symlinks quebrados).

---

### B. Melhorias Arquiteturais & Engenharia de Sistemas (Rust Alto Desempenho)

1. **Eliminação de Alocações Temporárias no Loop Hot-Path do Renderer (`results.rs`) [CONCLUÍDO]**:
   - *Status*: Implementado. Concatenações de `String` no render loop da `nav_bar` foram substituídas por `Span`s estáticos pré-computados e `&'static str` reutilizáveis.

2. **Scanning Assíncrono e Especulativo de Subdiretórios (Speculative Directory Scanning) [CONCLUÍDO]**:
   - *Status*: Implementado. Ao mover o cursor sobre uma pasta, um worker assíncrono Tokio pré-carrega o conteúdo da pasta em um cache LRU na RAM. Transição com `l` ocorre instantaneamente com **0ms de I/O latency**.

3. **Otimização de Carregamento de Imagens com Decodificação Assíncrona Off-thread [CONCLUÍDO]**:
   - *Status*: Implementado. Clonagem de `DynamicImage`, corte de pixels (`crop_imm`) e codificação do protocolo gráfico (`ratatui_image`) agora rodam off-thread via `tokio::task::spawn_blocking`, zerando micro-stutters no render loop.

4. **Macros Procedurais de Mesclagem Parcial (`matchmaker-partial-macros`)**:
   - *Análise de Código*: A proc-macro deriva structs parciais para sobreposição de configurações TOML.

---

## 4. Status de Implementação do Plano de Ação

1. **Fase 1 (Otimizações no Render Loop)**: [CONCLUÍDO]
   - Eliminar alocações em `results.rs` na construção das spans da `nav_bar`.
   - Implementar dicas visuais de teclas ativas no footer quando o modo `--nav` estiver focado na lista (`--nav-hints`).

2. **Fase 2 (Performance Async & Decodificação Off-Thread)**: [CONCLUÍDO]
   - Implementar pré-carregamento especulativo em background para o diretório no cursor (`SpeculativeDirCache`).
   - Migrar a codificação/corte de `ratatui_image` para um worker assíncrono isolado (`spawn_blocking`).

3. **Fase 3 (Navegação & Zero Fricção)**:
   - Desenvolver o wrapper shell `mmcd` para persistência de diretório no terminal.
   - Adicionar o layout de 3 painéis (Painel de navegação pai em formato minimizado).

---

*Documento atualizado automaticamente para o repositório Matchmaker.*
