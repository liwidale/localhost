import { app } from 'electron'
import { resolveLang, translate } from '../shared/i18n'
import { engine } from './engine'

export const tr = (key: string, ...args: unknown[]): string => translate(resolveLang(engine.settings?.language, app.getLocale()), key, args)
