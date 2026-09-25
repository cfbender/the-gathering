/**
 * The browser's IANA time zone. Sent with game and stats requests so the server reads
 * dates, weekdays, and hours on the same local calendar the charts show.
 */
export function browserTimeZone(): string | undefined {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || undefined
  } catch {
    return undefined
  }
}
