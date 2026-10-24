export class JsonUtils {
  static parse <T = any>(text?: string | null, reviver?: Parameters<typeof JSON.parse>[1], _default?: T): T {
    try {
      const parseReviver = reviver as ((this: any, key: string, value: any) => any) | undefined

      return text
        ? JSON.parse(text, parseReviver) as T
        : _default as T
    } catch (error) {
      console.warn('JsonUtils.parse error', { text, error })
      return _default as T
    }
  }
}
