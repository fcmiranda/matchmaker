# Análise: Integração `fff-search` + Matchmaker vs. Daemon Proposto

> **Contexto**: Avaliação técnica da viabilidade de usar o [`fff-search`](https://github.com/dmtrKovalenko/fff) (v0.10.3) como motor integrado ao matchmaker, comparado com a proposta de daemon nativo.

---

## 1. O Que o `fff-search` Realmente É

O `fff-search` (Fabulous & Fast File Finder) é um **SDK de busca de arquivos** desenhado para processos de longa duração (editores, AI agents). Características chave:

| Feature | fff-search | matchmaker (atual) |
|---|---|---|
| Indexação em memória | ✅ Trigram inverted index | ❌ Sem índice persistente |
| File watching | ✅ Built-in watcher | ❌ Re-scan por execução |
| Fuzzy matching | ✅ Trigram + typo tolerance | ✅ nucleo (SIMD Smith-Waterman) |
| Frecency | ✅ Built-in | ✅ Próprio (`redb` + `FrecencySnapshot`) |
| Git-aware | ✅ Boost modified/staged | ❌ |
| `.gitignore` | ✅ Via `ignore` crate | ❌ Depende do comando externo (`fd`) |
| Grep/content search | ✅ SIMD Aho-Corasick multi-grep | ❌ Delega para `rg` |
| TUI | ❌ **Nenhuma** (é SDK puro) | ✅ Ratatui completa |
| Pipe/filtro genérico | ❌ Apenas arquivos | ✅ Qualquer stream de texto |
| Dependências | `git2`, `heed`, `blake3`, `aho-corasick` (~pesado) | `nucleo` fork (~leve) |

### Observação Crucial

O fff usa **trigram-based matching**, que é fundamentalmente diferente do **Smith-Waterman fuzzy scoring** do nucleo. São algoritmos com trade-offs distintos:

- **Trigram** (fff): Melhor para typo tolerance ("maiin.rs" → "main.rs"), pré-filtragem rápida via índice invertido. Mas ranking menos preciso para queries curtas.
- **nucleo/Smith-Waterman** (matchmaker): Melhor ranking fuzzy fino, posicional scoring, contiguity bonus. O que faz o matchmaker "sentir" responsivo como fzf.

---

## 2. Análise dos 3 Modelos de Integração

### Modelo A: fff como Gerador de Lista (via TOML)
```toml
[start.command]
command = "fff list ."
```

**Veredicto**: ⚠️ **Não vale a pena.** Isso é funcionalmente idêntico a `command = "fd"`. O fff como CLI _ainda precisa_ spawnar um processo e serializar para stdout. O overhead de IPC-via-pipe permanece. A única vantagem seria se o fff-daemon já estivesse rodando e o CLI fosse um thin client, mas aí caímos no problema do daemon que já discutimos.

### Modelo B: fff como Motor de Busca Contínuo (query por tecla)
Cada keystroke envia a query para o fff, que devolve resultados ranqueados.

**Veredicto**: ❌ **Conflito fundamental com a arquitetura.** O matchmaker tem o nucleo rodando matching SIMD em background threads com snapshot incremental. Substituir isso por round-trips IPC para cada keystroke introduziria latência visível (~1-5ms por round-trip vs. ~50μs para nucleo local). O nucleo já faz matching em <1ms para 100K itens. Não há ganho.

### Modelo C: Integração nativa via Rust crate (feature flag)
```toml
[features]
fff-engine = ["dep:fff-search"]
```

**Veredicto**: ✅ **Este é o único modelo que faz sentido técnico**, mas com escopo limitado.

---

## 3. O Que Realmente Faz Sentido: fff como `ItemSource`, Não como `SearchEngine`

A conversa com a outra IA propõe um trait `SearchEngine` onde o fff substitui o nucleo. **Isso é um erro de design.** O fff e o nucleo resolvem problemas diferentes:

```
fff-search  = INDEXADOR de arquivos (trigram index + watcher + frecency)
nucleo      = MATCHER fuzzy (SIMD scoring + incremental snapshot)
```

O design correto é usar o fff **apenas como fonte de itens**, mantendo o nucleo para matching:

```
┌─────────────────────────────────┐
│         matchmaker-cli          │
│                                 │
│  ┌─────────┐    ┌────────────┐  │
│  │ fff-search│   │  stdin     │  │
│  │ (walker   │   │  (pipe)    │  │
│  │ + index)  │   │            │  │
│  └────┬──────┘   └─────┬──────┘  │
│       │                │         │
│       ▼                ▼         │
│  ┌──────────────────────────┐   │
│  │    nucleo::Worker        │   │
│  │    (fuzzy matching)      │   │
│  │    + FrecencySnapshot    │   │
│  └──────────────────────────┘   │
│       │                         │
│       ▼                         │
│  ┌──────────────────────────┐   │
│  │    Ratatui TUI           │   │
│  └──────────────────────────┘   │
└─────────────────────────────────┘
```

### O que o fff adicionaria nesse modelo

1. **Walker nativo sem subprocess** — substitui `fd`/`find` (elimina fork+exec)
2. **`.gitignore` awareness nativa** — sem depender do `fd`
3. **File watching incremental** — novos/deletados arquivos aparecem sem reload manual
4. **Git status boost** — arquivos modificados/staged sobem no ranking

### O que o matchmaker JÁ faz melhor que o fff e deve manter

1. **Fuzzy matching** — nucleo é superior em ranking fino
2. **Frecency** — o matchmaker já tem implementação própria com `redb` + `FrecencySnapshot`
3. **Pipe/filtro genérico** — fff não sabe lidar com texto arbitrário
4. **TUI completa** — o fff não tem interface

---

## 4. fff vs. Daemon vs. Alternativas: Comparação Final

| Critério | Daemon (proposto) | fff integrado | Walker nativo (`ignore` crate) | Cache `redb` |
|---|---|---|---|---|
| Complexidade | ⬆⬆⬆ | ⬆⬆ | ⬆ | ⬆ |
| Dependências adicionais | `notify`, `rkyv`, socket mgmt | `fff-search` (~8 deps: `git2`, `heed`, `blake3`...) | `ignore` (~1 dep leve) | Nenhuma (já usa `redb`) |
| Cold start improvement | ⬆⬆⬆ (se daemon já rodando) | ⬆⬆ (se index em memória) | ⬆⬆ (parallel walker) | ⬆⬆ (cached listing) |
| File watching | ✅ | ✅ | ❌ | ❌ |
| Compile time impact | Moderado | **Alto** (`git2` + `heed` = +30-60s) | Baixo (~5s) | Zero |
| Risco de manutenção | Alto (daemon lifecycle) | Médio (dep externa em evolução rápida v0.10) | Baixo (crate estável e madura) | Mínimo |
| Funciona sem daemon | ❌ (precisa do processo) | ✅ (in-process) | ✅ | ✅ |

---

## 5. Respondendo a Pergunta: Vale a Pena?

### ✅ Sim, **parcialmente** — mas não da forma proposta na conversa.

A conversa com a IA comete um erro fundamental: trata o fff como **substituto** do nucleo. Na verdade, o valor do fff para o matchmaker é como **walker + watcher**, não como search engine.

### Proposta concreta de integração

```toml
# matchmaker-lib/Cargo.toml
[features]
fff-walker = ["dep:fff-search"]  # Opcional, não default

[dependencies]
fff-search = { version = "0.10", optional = true }
```

```rust
// matchmaker-lib/src/walker/mod.rs
pub trait ItemSource: Send + Sync + 'static {
    fn stream_items(&self) -> impl Stream<Item = String> + Send;
    fn supports_watching(&self) -> bool { false }
}

// matchmaker-lib/src/walker/command.rs (atual, via subprocess)
pub struct CommandSource { /* fd/find/command */ }

// matchmaker-lib/src/walker/ignore_walker.rs (P0, leve)  
pub struct IgnoreWalker { /* ignore crate, parallel */ }

// matchmaker-lib/src/walker/fff.rs (P3, feature-gated)
#[cfg(feature = "fff-walker")]
pub struct FffSource { /* fff-search index + watcher */ }
```

### Mas... vale o custo?

O `fff-search` traz **~8 dependências transitivas pesadas** (`git2` sozinho compila com `libgit2` C bindings). Para um CLI tool que preza por build time rápido, isso é significativo.

A **crate `ignore`** (do mesmo autor do `ripgrep`) oferece 80% do benefício:
- Walker paralelo ✅
- `.gitignore` aware ✅  
- `.ignore` aware ✅
- Apenas 1 dependência leve ✅
- Maturíssima e estável ✅

O fff só ganha do `ignore` em 2 coisas: **file watching** e **git status boost**. Se esses não são prioridade imediata, `ignore` é suficiente.

---

## 6. Veredicto Combinado (Daemon + fff)

| Abordagem | Recomendação | Razão |
|---|---|---|
| Daemon nativo (PROMPT_DAEMON_IMPLEMENTATION.md) | ❌ Não agora | Complexidade operacional desproporcional |
| fff como SearchEngine (substitui nucleo) | ❌ Não | Conflito arquitetural, nucleo é superior para ranking fuzzy |
| fff como ItemSource via feature flag | ⚠️ Talvez depois | Válido mas pesado em deps. Considerar quando file watching for prioridade |
| Walker nativo com `ignore` crate | ✅ **Recomendado agora** | 80% do benefício, 10% da complexidade, sem deps pesadas |
| Cache de listing em `redb` | ✅ **Recomendado agora** | Warm start instantâneo, zero deps extras |

> [!TIP]
> **Caminho sugerido**: Implementar `ignore` walker (P0) → cache `redb` (P1) → se ainda precisar de file watching, **aí** avaliar `notify` crate sozinha (mais leve que fff inteiro) ou fff como feature opcional.

A conversa com a IA tem a intuição certa ("juntar o cérebro do fff com o rosto do matchmaker") mas erra no mecanismo. O matchmaker não precisa do _cérebro_ do fff — ele já tem o nucleo. Precisa apenas das _pernas_ (walker + watcher).
