import ru from './locales/ru'

export type Lang = 'en' | 'ru'

const DICTS: Record<Exclude<Lang, 'en'>, Record<string, string>> = { ru }

const PLURALS: Record<Lang, Intl.LDMLPluralRule[]> = {
  en: ['one', 'other'],
  ru: ['one', 'few', 'many', 'other'],
}

export const LANGS: Lang[] = ['en', 'ru']

export function resolveLang(setting: string | undefined, locale: string | undefined): Lang {
  const pick = setting && setting !== 'auto' ? setting : locale || 'en'
  const base = pick.toLowerCase().split(/[-_]/)[0]
  return LANGS.includes(base as Lang) ? (base as Lang) : 'en'
}

const rules = new Map<Lang, Intl.PluralRules>()

function plural(lang: Lang, forms: string[], n: number): string {
  let r = rules.get(lang)
  if (!r) {
    r = new Intl.PluralRules(lang)
    rules.set(lang, r)
  }
  const i = PLURALS[lang].indexOf(r.select(n))
  return forms[i >= 0 && i < forms.length ? i : forms.length - 1]
}

export function translate(lang: Lang, key: string, args: unknown[]): string {
  const raw = lang === 'en' ? key : DICTS[lang][key] || key
  const source = raw === key && lang !== 'en' ? 'en' : lang
  let text = raw
  if (raw.includes('|')) {
    const n = typeof args[0] === 'number' ? args[0] : 0
    text = plural(source, raw.split('|'), n)
  }
  return args.length ? text.replace(/\{(\d)\}/g, (m, d: string) => (args[Number(d)] === undefined ? m : String(args[Number(d)]))) : text
}
