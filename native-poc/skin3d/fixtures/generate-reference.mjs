// Run: node fixtures/generate-reference.mjs /path/to/node_modules/skinview3d
// Uses only upstream model/animation math; no DOM, window, canvas or WebGL.
import { createRequire } from 'node:module'
import { pathToFileURL, fileURLToPath } from 'node:url'
import { resolve, dirname } from 'node:path'
import { writeFileSync, realpathSync } from 'node:fs'

const root = realpathSync(resolve(process.argv[2]))
const require = createRequire(resolve(root, 'package.json'))
const { Vector3, Matrix3 } = await import(pathToFileURL(require.resolve('three')))
const { PlayerObject } = await import(pathToFileURL(resolve(root, 'libs/model.js')))
const { WalkingAnimation } = await import(pathToFileURL(resolve(root, 'libs/animation.js')))
const lines = ['# skinview3d 3.4.2 / three 0.156.1: model(0=classic,1=slim),time,part,vertex,x,y,z,nx,ny,nz,u,v(top-left)']
for (const [model, type] of ['default', 'slim'].entries()) {
  for (const time of [0, 0.75, 2]) {
    const player = new PlayerObject()
    player.skin.modelType = type
    const animation = new WalkingAnimation()
    animation.progress = time * 0.6
    animation.animate(player)
    player.rotation.y = time * 0.6
    player.updateMatrixWorld(true)
    const meshes = player.skin.getBodyParts().flatMap(p => [p.innerLayer, p.outerLayer])
    meshes.push(player.cape.cape)
    for (const [part, mesh] of meshes.entries()) {
      const normalMatrix = new Matrix3().getNormalMatrix(mesh.matrixWorld)
      for (const vertex of [0, 6, 12, 18, 24, 30]) {
        const index = mesh.geometry.index.getX(vertex)
        const p = new Vector3().fromBufferAttribute(mesh.geometry.attributes.position, index).applyMatrix4(mesh.matrixWorld)
        const n = new Vector3().fromBufferAttribute(mesh.geometry.attributes.normal, index).applyMatrix3(normalMatrix).normalize()
        const uv = mesh.geometry.attributes.uv
        lines.push([model, time, part, vertex, ...p, ...n, uv.getX(index), 1 - uv.getY(index)].join(','))
      }
    }
  }
}
writeFileSync(resolve(dirname(fileURLToPath(import.meta.url)), 'upstream-vertices.csv'), lines.join('\n') + '\n')
