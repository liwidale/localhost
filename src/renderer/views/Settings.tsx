import type { Settings as SettingsValue, Theme } from '../../shared/types'
import iconUrl from '../assets/icon.png'
import { tr } from '../shared/i18n'
import { Btn, Group, Row, Seg, Select, Toggle } from '../ui'

const api = window.localhost

export function Settings({ s }: { s: SettingsValue | null }) {
  if (!s) return null
  const patch = (p: Partial<SettingsValue>): void => void api.setSettings(p)
  const themes: { value: Theme; label: string }[] = [
    { value: 'system', label: tr('Automatic') },
    { value: 'light', label: tr('Light') },
    { value: 'dark', label: tr('Dark') },
  ]
  return (
    <>
      <Group title={tr('Services')}>
        <Row title={tr('Show system services')} desc={tr('Services owned by the operating system or other users.')}>
          <Toggle label={tr('Show system services')} on={s.showSystem} onChange={(v) => patch({ showSystem: v })} />
        </Row>
        <Row title={tr('Check service health')} desc={tr('Sends one lightweight request to local services when they appear and when you open them.')}>
          <Toggle label={tr('Check service health')} on={s.healthChecks} onChange={(v) => patch({ healthChecks: v })} />
        </Row>
        <Row
          title={tr('Notify when a service stops unexpectedly')}
          desc={tr('Only for services started by Localhost, and only while the window is in the background.')}
        >
          <Toggle label={tr('Notifications')} on={s.notifications} onChange={(v) => patch({ notifications: v })} />
        </Row>
        <Row title={tr('Language')}>
          <Select
            label={tr('Language')}
            value={s.language}
            onChange={(v) => patch({ language: v })}
            options={[
              { value: 'auto', label: tr('System') },
              { value: 'en', label: 'English' },
              { value: 'ru', label: 'Русский' },
            ]}
          />
        </Row>
      </Group>
      <Group title={tr('Theme')}>
        <div className="theme-picker">
          {themes.map((t) => (
            <button
              key={t.value}
              className={`theme-card${s.theme === t.value ? ' on' : ''}`}
              onClick={() => patch({ theme: t.value })}
              aria-pressed={s.theme === t.value}
            >
              <span className={`theme-preview tp-${t.value}`}>
                <span className="tp-pill">
                  <span className="tp-dot" />
                  <span className="tp-line" />
                </span>
              </span>
              <span className="theme-label">{t.label}</span>
            </button>
          ))}
        </div>
      </Group>
      <Group title={tr('Motion')}>
        <Row title={tr('Motion')} desc={tr('Reduced keeps transitions short and still.')}>
          <Seg
            label={tr('Motion')}
            value={s.motion}
            onChange={(v) => patch({ motion: v })}
            options={[
              { value: 'full', label: tr('Full') },
              { value: 'reduced', label: tr('Reduced') },
            ]}
          />
        </Row>
      </Group>
      <div className="about">
        <img src={iconUrl} alt="" />
        <div className="about-name">Localhost</div>
        <div className="about-version">{tr('Version {0}', api.version)}</div>
        <div className="about-line">{tr('Understand and control your local development environment.')}</div>
        <div className="about-actions">
          <Btn kind="ghost" onClick={() => window.close()}>
            {tr('Quit Localhost')}
          </Btn>
        </div>
      </div>
    </>
  )
}
