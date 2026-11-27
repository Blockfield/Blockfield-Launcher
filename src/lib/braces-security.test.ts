import { createRequire } from 'node:module'
import { describe, expect, it } from 'vitest'

const require = createRequire(import.meta.url)
const requireStylelint = createRequire(require.resolve('stylelint'))
const requireMicromatch = createRequire(requireStylelint.resolve('micromatch'))
const braces = requireMicromatch('braces') as {
  parse: (input: string) => unknown
  compile: (input: string) => string
  expand: (input: string) => string[]
  stringify: (input: string) => string
}
const micromatch = requireStylelint('micromatch') as (paths: string[], pattern: string) => string[]

describe('Stylelint brace expansion security patch', () => {
  it.each([
    ['{', '}'],
    ['(', ')'],
    ['{(', ')}'],
  ])('rejects excessive nesting of %s before recursive AST walkers run', (open, close) => {
    const depth = Math.floor(4000 / open.length)
    const pattern = open.repeat(depth) + 'x' + close.repeat(depth)
    expect(pattern.length).toBeLessThan(10000)
    for (const run of [braces.parse, braces.compile, braces.expand, braces.stringify]) {
      expect(() => run(pattern)).toThrow('Input nesting depth exceeds max depth (100)')
    }
  })

  it('preserves nested alternatives, numeric ranges, and Stylelint CSS globs', () => {
    expect(braces.expand('src/{app,{lib,styles}}/{1..2}.css')).toEqual([
      'src/app/1.css',
      'src/app/2.css',
      'src/lib/1.css',
      'src/lib/2.css',
      'src/styles/1.css',
      'src/styles/2.css',
    ])
    expect(
      micromatch(
        ['src/app/theme.css', 'src/styles/global.css', 'src/lib/api.ts'],
        'src/{app,styles}/**/*.css',
      ),
    ).toEqual(['src/app/theme.css', 'src/styles/global.css'])
    const escaped = '\\{'.repeat(200) + 'x' + '\\}'.repeat(200)
    expect(braces.compile(escaped)).toBe('{'.repeat(200) + 'x' + '}'.repeat(200))
    const quoted = '"' + '{'.repeat(200) + 'x' + '}'.repeat(200) + '"'
    expect(braces.compile(quoted)).toBe('{'.repeat(200) + 'x' + '}'.repeat(200))
    expect(() => braces.compile('{'.repeat(100) + 'x' + '}'.repeat(100))).not.toThrow()
  })
})
