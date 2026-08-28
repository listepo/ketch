/**
 * Shell completion scripts, generated from the live command surface.
 *
 * Derived from the `Command` tree rather than written out by hand, so a
 * subcommand or flag added in `cli.ts` is completable without anyone
 * remembering to update a script here. The scripts complete installed package
 * names by shelling back out to `ketch list --names-only`, which is the one
 * thing a statically generated script cannot do.
 */

import type { Command } from "@commander-js/extra-typings";

/** The shells ketch generates for — the same three `shell.ts` can set up. */
export const COMPLETION_SHELLS = ["bash", "zsh", "fish"] as const;

/** One of {@link COMPLETION_SHELLS}. */
export type CompletionShell = (typeof COMPLETION_SHELLS)[number];

/**
 * Commands whose positional argument is an installed package.
 *
 * `install` is absent on purpose: what it takes is a repository that has not
 * been installed yet, and no local list can complete that.
 */
const TAKES_INSTALLED = new Set([
  "uninstall",
  "upgrade",
  "pin",
  "unpin",
  "link",
  "unlink",
  "info",
  "changelog",
]);

/** A command flattened to what a completion script needs to know about it. */
interface Node {
  readonly names: readonly string[];
  readonly description: string;
  readonly flags: readonly string[];
  readonly subs: readonly Node[];
  readonly installed: boolean;
}

/** Flatten one command, and its children, into {@link Node}s. */
function describe(command: Command, inherited: readonly string[]): Node {
  const flags = [
    ...command.options.flatMap((option) =>
      [option.short, option.long].filter((f): f is string => f !== undefined),
    ),
    ...inherited,
  ];
  return {
    names: [command.name(), ...command.aliases()],
    description: command.description(),
    flags,
    subs: command.commands.map((sub) => describe(sub as Command, inherited)),
    installed: TAKES_INSTALLED.has(command.name()),
  };
}

/** The global flags, which clap marked `global = true` and every command takes. */
function globals(program: Command): string[] {
  return program.options.flatMap((option) =>
    [option.short, option.long].filter((f): f is string => f !== undefined),
  );
}

/** Escape for a POSIX single-quoted string. */
function q(text: string): string {
  return text.replaceAll("'", "'\\''");
}

/** Render a completion script for `shell`. */
export function completions(program: Command, shell: CompletionShell): string {
  const inherited = [...globals(program), "-h", "--help"];
  const nodes = program.commands.map((c) => describe(c as Command, inherited));
  switch (shell) {
    case "bash":
      return bash(nodes);
    case "zsh":
      return zsh(nodes);
    case "fish":
      return fish(nodes);
  }
}

function bash(nodes: readonly Node[]): string {
  const top = nodes.flatMap((n) => n.names).join(" ");
  const flagArms = nodes
    .map((n) => `    ${n.names.join("|")}) opts="${n.flags.join(" ")}" ;;`)
    .join("\n");
  const subArms = nodes
    .filter((n) => n.subs.length > 0)
    .map((n) => `    ${n.names.join("|")}) subs="${n.subs.flatMap((s) => s.names).join(" ")}" ;;`)
    .join("\n");
  const installed = nodes
    .filter((n) => n.installed)
    .flatMap((n) => n.names)
    .join("|");
  return `# ketch completion for bash — source it, or drop it in your bash-completion dir.
_ketch() {
  local cur cmd opts subs
  cur="\${COMP_WORDS[COMP_CWORD]}"
  cmd="\${COMP_WORDS[1]}"
  COMPREPLY=()

  if [ "$COMP_CWORD" -eq 1 ]; then
    COMPREPLY=( $(compgen -W "${top}" -- "$cur") )
    return
  fi

  if [[ "$cur" == -* ]]; then
    case "$cmd" in
${flagArms}
    esac
    COMPREPLY=( $(compgen -W "$opts" -- "$cur") )
    return
  fi

  if [ "$COMP_CWORD" -eq 2 ]; then
    case "$cmd" in
${subArms}
    esac
    if [ -n "$subs" ]; then
      COMPREPLY=( $(compgen -W "$subs" -- "$cur") )
      return
    fi
  fi

  case "$cmd" in
    ${installed}) COMPREPLY=( $(compgen -W "$(ketch list --names-only 2>/dev/null)" -- "$cur") ) ;;
  esac
}
complete -F _ketch ketch
`;
}

function zsh(nodes: readonly Node[]): string {
  const top = nodes
    .map((n) => `    '${q(n.names[0] ?? "")}:${q(n.description.replaceAll(":", " -"))}'`)
    .join("\n");
  const flagArms = nodes
    .map((n) => `      ${n.names.join("|")}) opts=(${n.flags.map((f) => `'${f}'`).join(" ")}) ;;`)
    .join("\n");
  const subArms = nodes
    .filter((n) => n.subs.length > 0)
    .map(
      (n) =>
        `      ${n.names.join("|")}) subs=(${n.subs
          .map((s) => `'${q(s.names[0] ?? "")}:${q(s.description.replaceAll(":", " -"))}'`)
          .join(" ")}) ;;`,
    )
    .join("\n");
  const installed = nodes
    .filter((n) => n.installed)
    .flatMap((n) => n.names)
    .join("|");
  return `#compdef ketch
# ketch completion for zsh — put it on your $fpath as _ketch.
_ketch() {
  local -a cmds opts subs
  cmds=(
${top}
  )

  if (( CURRENT == 2 )); then
    _describe 'command' cmds
    return
  fi

  local cmd=\${words[2]}

  if [[ \${words[CURRENT]} == -* ]]; then
    case $cmd in
${flagArms}
    esac
    compadd -- $opts
    return
  fi

  if (( CURRENT == 3 )); then
    case $cmd in
${subArms}
    esac
    if (( \${#subs} )); then
      _describe 'subcommand' subs
      return
    fi
  fi

  case $cmd in
    ${installed}) compadd -- \${(f)"$(ketch list --names-only 2>/dev/null)"} ;;
  esac
}
compdef _ketch ketch
`;
}

function fish(nodes: readonly Node[]): string {
  const lines: string[] = [
    "# ketch completion for fish — put it in ~/.config/fish/completions/ketch.fish.",
    "complete -c ketch -f",
  ];
  for (const node of nodes) {
    for (const name of node.names) {
      lines.push(
        `complete -c ketch -n __fish_use_subcommand -a ${name} -d '${q(node.description)}'`,
      );
    }
    const seen = `__fish_seen_subcommand_from ${node.names.join(" ")}`;
    for (const flag of node.flags) {
      const kind = flag.startsWith("--") ? "-l" : "-s";
      lines.push(`complete -c ketch -n '${seen}' ${kind} ${flag.replace(/^-+/, "")}`);
    }
    for (const sub of node.subs) {
      for (const name of sub.names) {
        lines.push(`complete -c ketch -n '${seen}' -a ${name} -d '${q(sub.description)}'`);
      }
    }
    if (node.installed) {
      lines.push(`complete -c ketch -n '${seen}' -a '(ketch list --names-only)'`);
    }
  }
  return `${lines.join("\n")}\n`;
}
