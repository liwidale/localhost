import type { CSSProperties, ReactElement } from 'react'

const P: Record<string, ReactElement> = {
  back: <path d="M10 3.5 5.5 8l4.5 4.5" />,
  forward: <path d="M6 3.5 10.5 8 6 12.5" />,
  down: <path d="M3.5 6 8 10.5 12.5 6" />,
  up: <path d="M3.5 10 8 5.5l4.5 4.5" />,
  jump: (
    <>
      <path d="M6 3.5h6.5V10" />
      <path d="M12.5 3.5 4 12" />
    </>
  ),
  plus: <path d="M8 3v10M3 8h10" />,
  close: <path d="M4 4l8 8M12 4l-8 8" />,
  check: <path d="M3.2 8.4 6.4 11.5 12.8 4.8" />,
  gear: (
    <>
      <path d="M2.5 5h6.2M12.3 5h1.2M2.5 11h1.2M7.3 11h6.2" />
      <circle cx="10.5" cy="5" r="1.7" />
      <circle cx="5.5" cy="11" r="1.7" />
    </>
  ),
  chat: <path d="M3 4.6c0-1 .8-1.8 1.8-1.8h6.4c1 0 1.8.8 1.8 1.8v4.6c0 1-.8 1.8-1.8 1.8H7.4L4.6 13.2V11H4.8c-1 0-1.8-.8-1.8-1.8z" />,
  sessions: (
    <>
      <rect x="2.5" y="3" width="11" height="4" rx="1.4" />
      <rect x="2.5" y="9" width="11" height="4" rx="1.4" />
    </>
  ),
  clip: <path d="M12.6 7.6 8.1 12.1a2.8 2.8 0 0 1-4-4L8.8 3.4a1.9 1.9 0 0 1 2.7 2.7L6.9 10.7a.95.95 0 0 1-1.35-1.35L9.7 5.2" />,
  folder: <path d="M2.5 4.8c0-.7.6-1.3 1.3-1.3h2.6l1.4 1.5h4.4c.7 0 1.3.6 1.3 1.3v5.4c0 .7-.6 1.3-1.3 1.3H3.8c-.7 0-1.3-.6-1.3-1.3z" />,
  file: (
    <>
      <path d="M4 2.5h5l3 3v8H4z" />
      <path d="M9 2.5v3h3" />
    </>
  ),
  image: (
    <>
      <rect x="2.5" y="3" width="11" height="10" rx="1.6" />
      <circle cx="6" cy="6.5" r="1.1" />
      <path d="M13.5 10.5 10.4 7.6 4.5 13" />
    </>
  ),
  window: (
    <>
      <rect x="2" y="3" width="12" height="10" rx="1.6" />
      <path d="M2 6h12" />
      <path d="M4.2 4.5h.01M5.9 4.5h.01" />
    </>
  ),
  text: <path d="M3 4h10M3 7h10M3 10h6.5M3 13h4" />,
  terminal: (
    <>
      <rect x="2" y="3" width="12" height="10" rx="1.6" />
      <path d="M4.8 6.4 6.8 8l-2 1.6M8.4 10h2.8" />
    </>
  ),
  alert: (
    <>
      <path d="M8 2.6 14 13H2z" />
      <path d="M8 6.6v2.8M8 11.2h.01" />
    </>
  ),
  stop: <rect x="4.5" y="4.5" width="7" height="7" rx="1.4" fill="currentColor" stroke="none" />,
  send: <path d="M8 13V3.5M4 7.3 8 3.3l4 4" />,
  copy: (
    <>
      <rect x="5.5" y="5.5" width="8" height="8" rx="1.6" />
      <path d="M10.5 5.5V3.8c0-.7-.6-1.3-1.3-1.3H3.8c-.7 0-1.3.6-1.3 1.3v5.4c0 .7.6 1.3 1.3 1.3h1.7" />
    </>
  ),
  trash: <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.2c0 .5.5.8 1 .8h3.8c.5 0 1-.3 1-.8l.6-8.2" />,
  branch: (
    <>
      <circle cx="5" cy="3.8" r="1.4" />
      <circle cx="5" cy="12.2" r="1.4" />
      <circle cx="11" cy="6" r="1.4" />
      <path d="M5 5.2v5.6M11 7.4c0 2.2-2.2 2.6-6 3.4" />
    </>
  ),
  edit: <path d="M10.4 3.2 12.8 5.6 6 12.4l-3 .6.6-3z" />,
  search: (
    <>
      <circle cx="7" cy="7" r="4" />
      <path d="M10 10l3 3" />
    </>
  ),
  globe: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M2.5 8h11M8 2.5c1.6 1.6 2.3 3.4 2.3 5.5S9.6 11.9 8 13.5C6.4 11.9 5.7 10.1 5.7 8S6.4 4.1 8 2.5z" />
    </>
  ),
  clock: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 5v3.2l2 1.3" />
    </>
  ),
  shield: <path d="M8 2.3 13 4v4c0 3-2.2 5-5 5.8C5.2 13 3 11 3 8V4z" />,
  refresh: <path d="M12.8 6.2A5 5 0 1 0 13 9M13 3v3.4H9.6" />,
  history: (
    <>
      <path d="M3 8a5 5 0 1 0 1.5-3.6M3 2.8v2.4h2.4" />
      <path d="M8 5.5v2.8l1.8 1.1" />
    </>
  ),
  more: (
    <>
      <circle cx="3.8" cy="8" r=".9" fill="currentColor" />
      <circle cx="8" cy="8" r=".9" fill="currentColor" />
      <circle cx="12.2" cy="8" r=".9" fill="currentColor" />
    </>
  ),
  bolt: <path d="M9 2 3.8 9h3.6L7 14l5.2-7H8.6z" />,
  layers: (
    <>
      <path d="M8 2.5 14 5.5 8 8.5 2 5.5z" />
      <path d="M2 8.4l6 3 6-3M2 11l6 3 6-3" />
    </>
  ),
  inbox: (
    <>
      <path d="M2.5 9.5 4.2 3.8c.2-.5.6-.8 1.1-.8h5.4c.5 0 .9.3 1.1.8l1.7 5.7v2.8c0 .7-.6 1.2-1.2 1.2H3.7c-.6 0-1.2-.5-1.2-1.2z" />
      <path d="M2.5 9.5h3l1 1.5h3l1-1.5h3" />
    </>
  ),
  link: <path d="M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2-2a2.6 2.6 0 0 0-3.7-3.7l-.6.6M9.2 6.8a2.6 2.6 0 0 0-3.7 0l-2 2a2.6 2.6 0 0 0 3.7 3.7l.6-.6" />,
  pause: <path d="M5.5 3.8v8.4M10.5 3.8v8.4" />,
  sparkle: <path d="M8 2.2c.4 2.9 1.5 4.3 4.6 4.8-3.1.5-4.2 1.9-4.6 4.8-.4-2.9-1.5-4.3-4.6-4.8 3.1-.5 4.2-1.9 4.6-4.8z" />,
  eye: (
    <>
      <path d="M1.8 8S4 3.8 8 3.8 14.2 8 14.2 8 12 12.2 8 12.2 1.8 8 1.8 8z" />
      <circle cx="8" cy="8" r="1.8" />
    </>
  ),
  minus: <path d="M3.5 8h9" />,
  play: <path d="M5.2 3.4v9.2l7.2-4.6z" fill="currentColor" stroke="none" />,
  cpu: (
    <>
      <rect x="4" y="4" width="8" height="8" rx="1.6" />
      <path d="M6.5 2v2M9.5 2v2M6.5 12v2M9.5 12v2M2 6.5h2M2 9.5h2M12 6.5h2M12 9.5h2" />
    </>
  ),
}

export type IconName = keyof typeof P

export function Icon({
  name,
  size = 16,
  style,
  className,
  strokeWidth = 1.5,
}: {
  name: IconName
  size?: number
  style?: CSSProperties
  className?: string
  strokeWidth?: number
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={style}
      className={className}
      aria-hidden
    >
      {P[name]}
    </svg>
  )
}
