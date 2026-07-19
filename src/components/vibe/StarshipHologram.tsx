import { useEffect, useRef, useState } from "react";
import type { BufferGeometry, Material, Object3D, Vector3Tuple } from "three";

type StarshipHologramProps = {
  className?: string;
  label: string;
};

type DisposableMaterial = {
  dispose?: () => void;
};

type DisposableGeometry = {
  dispose?: () => void;
};

type DisposableSceneObject = Object3D & {
  geometry?: DisposableGeometry;
  material?: DisposableMaterial | DisposableMaterial[];
};

type HologramMeshOptions = {
  edgeColor?: number;
  edgeOpacity?: number;
  edgeThreshold?: number;
  edges?: boolean;
  position?: Vector3Tuple;
  rotation?: Vector3Tuple;
  scale?: Vector3Tuple;
};

function disposeObject(object: Object3D) {
  object.traverse((child) => {
    const disposable = child as DisposableSceneObject;
    disposable.geometry?.dispose?.();

    if (Array.isArray(disposable.material)) {
      disposable.material.forEach((material) => material.dispose?.());
      return;
    }

    disposable.material?.dispose?.();
  });
}

export function StarshipHologram({ className = "", label }: StarshipHologramProps) {
  const hostRef = useRef<HTMLSpanElement | null>(null);
  const [webglReady, setWebglReady] = useState(false);
  const [webglFailed, setWebglFailed] = useState(false);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return;
    }

    const hasWebGlApi =
      typeof window.WebGLRenderingContext !== "undefined" ||
      typeof window.WebGL2RenderingContext !== "undefined";
    if (!hasWebGlApi) {
      setWebglFailed(true);
      return;
    }

    let cancelled = false;
    let frameId = 0;
    let resizeObserver: ResizeObserver | null = null;
    let removeWindowResize: (() => void) | null = null;
    let cleanupScene: (() => void) | null = null;

    async function init() {
      try {
        const THREE = await import("three");
        const hostElement = hostRef.current;
        if (cancelled || !hostElement) {
          return;
        }

        const renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true });
        renderer.setClearColor(0x000000, 0);
        renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
        renderer.domElement.className = "vibe-skin-space-ship-webgl-canvas";
        hostElement.appendChild(renderer.domElement);

        const scene = new THREE.Scene();
        const camera = new THREE.PerspectiveCamera(34, 1, 0.1, 20);
        camera.position.set(0.1, 0.98, 6.15);
        camera.lookAt(0, 0.08, 0);

        scene.add(new THREE.AmbientLight(0x7cf6ff, 1.35));
        const cyanLight = new THREE.PointLight(0x2ee8ff, 2.6, 6);
        cyanLight.position.set(-1.7, 1.4, 2.8);
        scene.add(cyanLight);
        const amberLight = new THREE.PointLight(0xf8c76a, 1.1, 4.6);
        amberLight.position.set(1.3, 0.6, 2.4);
        scene.add(amberLight);

        const shipGroup = new THREE.Group();
        shipGroup.scale.set(0.74, 0.74, 0.74);
        scene.add(shipGroup);

        const hullMaterial = new THREE.MeshStandardMaterial({
          blending: THREE.AdditiveBlending,
          color: 0x45efff,
          depthWrite: false,
          emissive: 0x0ea8bd,
          emissiveIntensity: 0.78,
          metalness: 0.2,
          opacity: 0.18,
          roughness: 0.16,
          side: THREE.DoubleSide,
          transparent: true,
        });
        const deckMaterial = new THREE.MeshStandardMaterial({
          blending: THREE.AdditiveBlending,
          color: 0x8dfbff,
          depthWrite: false,
          emissive: 0x2ee8ff,
          emissiveIntensity: 0.72,
          metalness: 0.14,
          opacity: 0.13,
          roughness: 0.2,
          side: THREE.DoubleSide,
          transparent: true,
        });
        const bridgeMaterial = new THREE.MeshStandardMaterial({
          blending: THREE.AdditiveBlending,
          color: 0xc8feff,
          depthWrite: false,
          emissive: 0x7cf6ff,
          emissiveIntensity: 0.92,
          opacity: 0.2,
          side: THREE.DoubleSide,
          transparent: true,
        });
        const engineMaterial = new THREE.MeshStandardMaterial({
          blending: THREE.AdditiveBlending,
          color: 0x2ee8ff,
          depthWrite: false,
          emissive: 0x2ee8ff,
          emissiveIntensity: 1.15,
          opacity: 0.28,
          transparent: true,
        });
        const glowMaterial = new THREE.MeshBasicMaterial({
          blending: THREE.AdditiveBlending,
          color: 0xf8c76a,
          depthWrite: false,
          opacity: 0.58,
          transparent: true,
        });
        const plumeMaterial = new THREE.MeshBasicMaterial({
          blending: THREE.AdditiveBlending,
          color: 0x7cf6ff,
          depthWrite: false,
          opacity: 0.34,
          transparent: true,
        });
        const panelMaterial = new THREE.LineBasicMaterial({
          blending: THREE.AdditiveBlending,
          color: 0xf8c76a,
          depthWrite: false,
          opacity: 0.56,
          transparent: true,
        });

        const applyTransform = (object: Object3D, options: HologramMeshOptions) => {
          if (options.position) {
            object.position.set(...options.position);
          }
          if (options.rotation) {
            object.rotation.set(...options.rotation);
          }
          if (options.scale) {
            object.scale.set(...options.scale);
          }
        };

        const addHologramMesh = (
          geometry: BufferGeometry,
          material: Material,
          options: HologramMeshOptions = {},
        ) => {
          const mesh = new THREE.Mesh(geometry, material);
          mesh.renderOrder = 1;
          applyTransform(mesh, options);
          shipGroup.add(mesh);

          if (options.edges !== false) {
            const edgeLine = new THREE.LineSegments(
              new THREE.EdgesGeometry(geometry, options.edgeThreshold ?? 18),
              new THREE.LineBasicMaterial({
                blending: THREE.AdditiveBlending,
                color: options.edgeColor ?? 0xb8fcff,
                depthWrite: false,
                opacity: options.edgeOpacity ?? 0.86,
                transparent: true,
              }),
            );
            edgeLine.renderOrder = 2;
            applyTransform(edgeLine, options);
            shipGroup.add(edgeLine);
          }

          return mesh;
        };

        const addPanelLines = (points: Vector3Tuple[]) => {
          const geometry = new THREE.BufferGeometry().setFromPoints(
            points.map(([x, y, z]) => new THREE.Vector3(x, y, z)),
          );
          const lines = new THREE.LineSegments(geometry, panelMaterial);
          lines.renderOrder = 3;
          shipGroup.add(lines);
        };

        const createFacetedHullGeometry = (
          sections: Array<{ x: number; width: number; height: number }>,
        ) => {
          const vertices: number[] = [];
          const indices: number[] = [];
          const sectionVertexCount = 6;

          sections.forEach(({ x, width, height }) => {
            vertices.push(
              x,
              height,
              0,
              x,
              height * 0.38,
              width,
              x,
              -height * 0.92,
              width * 0.72,
              x,
              -height,
              0,
              x,
              -height * 0.92,
              -width * 0.72,
              x,
              height * 0.38,
              -width,
            );
          });

          for (let section = 0; section < sections.length - 1; section += 1) {
            const current = section * sectionVertexCount;
            const next = (section + 1) * sectionVertexCount;
            for (let point = 0; point < sectionVertexCount; point += 1) {
              const pointNext = (point + 1) % sectionVertexCount;
              indices.push(
                current + point,
                current + pointNext,
                next + point,
                current + pointNext,
                next + pointNext,
                next + point,
              );
            }
          }

          for (let point = 1; point < sectionVertexCount - 1; point += 1) {
            indices.push(0, point, point + 1);
          }

          const last = (sections.length - 1) * sectionVertexCount;
          for (let point = 1; point < sectionVertexCount - 1; point += 1) {
            indices.push(last, last + point + 1, last + point);
          }

          const geometry = new THREE.BufferGeometry();
          geometry.setAttribute("position", new THREE.Float32BufferAttribute(vertices, 3));
          geometry.setIndex(indices);
          geometry.computeVertexNormals();
          return geometry;
        };

        const createWingGeometry = (side: -1 | 1) => {
          const z = side;
          const vertices = new Float32Array([
            -0.96,
            -0.04,
            z * 0.4,
            0.9,
            -0.08,
            z * 0.58,
            1.42,
            -0.12,
            z * 1.08,
            -0.34,
            -0.06,
            z * 1.18,
          ]);
          const geometry = new THREE.BufferGeometry();
          geometry.setAttribute("position", new THREE.BufferAttribute(vertices, 3));
          geometry.setIndex([0, 1, 2, 0, 2, 3]);
          geometry.computeVertexNormals();
          return geometry;
        };

        addHologramMesh(
          createFacetedHullGeometry([
            { x: -2.08, width: 0.035, height: 0.04 },
            { x: -1.54, width: 0.38, height: 0.16 },
            { x: -0.42, width: 0.68, height: 0.23 },
            { x: 0.72, width: 0.8, height: 0.25 },
            { x: 1.44, width: 0.62, height: 0.23 },
            { x: 1.78, width: 0.42, height: 0.18 },
          ]),
          hullMaterial,
          {
            edgeOpacity: 0.98,
          },
        );
        addHologramMesh(
          createFacetedHullGeometry([
            { x: -0.86, width: 0.18, height: 0.035 },
            { x: -0.18, width: 0.28, height: 0.055 },
            { x: 0.64, width: 0.34, height: 0.075 },
            { x: 1.12, width: 0.24, height: 0.052 },
          ]),
          deckMaterial,
          {
            edgeColor: 0xf8c76a,
            edgeOpacity: 0.66,
            position: [0, 0.27, 0],
          },
        );
        addHologramMesh(new THREE.BoxGeometry(0.54, 0.16, 0.34, 2, 1, 1), bridgeMaterial, {
          edgeOpacity: 0.88,
          position: [0.48, 0.48, 0],
          rotation: [0, 0, -0.04],
        });
        addHologramMesh(new THREE.BoxGeometry(0.32, 0.14, 0.26, 1, 1, 1), bridgeMaterial, {
          edgeColor: 0xf8c76a,
          edgeOpacity: 0.68,
          position: [0.72, 0.64, 0],
        });
        addHologramMesh(new THREE.CylinderGeometry(0.24, 0.3, 0.08, 40), bridgeMaterial, {
          edgeOpacity: 0.7,
          position: [0.58, 0.76, 0],
        });
        addHologramMesh(new THREE.TorusGeometry(0.28, 0.011, 8, 80), bridgeMaterial, {
          edgeOpacity: 0.58,
          position: [0.58, 0.84, 0],
          rotation: [Math.PI / 2, 0, 0],
        });
        addHologramMesh(new THREE.CylinderGeometry(0.008, 0.008, 0.54, 8), bridgeMaterial, {
          edgeOpacity: 0.45,
          position: [0.58, 1.1, 0],
        });
        addHologramMesh(new THREE.SphereGeometry(0.13, 24, 12), bridgeMaterial, {
          edgeColor: 0xffffff,
          edgeOpacity: 0.7,
          position: [-1.35, 0.16, 0],
          scale: [1.7, 0.52, 1.1],
        });
        addHologramMesh(new THREE.CylinderGeometry(0.018, 0.018, 0.86, 10), bridgeMaterial, {
          edgeOpacity: 0.58,
          position: [-1.88, 0.02, 0],
          rotation: [0, 0, Math.PI / 2],
        });

        [-1, 1].forEach((side) => {
          addHologramMesh(createWingGeometry(side as -1 | 1), deckMaterial, {
            edgeColor: 0x7cf6ff,
            edgeOpacity: 0.78,
          });
          addHologramMesh(new THREE.CylinderGeometry(0.11, 0.16, 1.82, 24), deckMaterial, {
            edgeOpacity: 0.76,
            position: [0.08, -0.08, side * 0.9],
            rotation: [0, 0, Math.PI / 2],
          });
          addHologramMesh(new THREE.SphereGeometry(0.11, 18, 10), bridgeMaterial, {
            edgeColor: 0xffffff,
            edgeOpacity: 0.58,
            position: [-0.88, -0.08, side * 0.9],
            scale: [1.4, 0.72, 0.72],
          });
          addHologramMesh(new THREE.CylinderGeometry(0.22, 0.31, 0.42, 40), engineMaterial, {
            edgeOpacity: 0.92,
            position: [1.78, -0.04, side * 0.47],
            rotation: [0, 0, Math.PI / 2],
          });
          addHologramMesh(new THREE.TorusGeometry(0.31, 0.03, 12, 80), engineMaterial, {
            edgeColor: 0xffffff,
            edgeOpacity: 0.92,
            position: [2, -0.04, side * 0.47],
            rotation: [0, Math.PI / 2, 0],
          });
          addHologramMesh(new THREE.ConeGeometry(0.2, 0.68, 36, 1, true), plumeMaterial, {
            edgeColor: 0x7cf6ff,
            edgeOpacity: 0.42,
            position: [2.36, -0.04, side * 0.47],
            rotation: [0, 0, -Math.PI / 2],
          });
          addHologramMesh(new THREE.SphereGeometry(0.07, 18, 10), glowMaterial, {
            edgeColor: 0xf8c76a,
            edgeOpacity: 0.36,
            position: [-1.52, 0.02, side * 0.26],
            scale: [1.2, 0.7, 0.7],
          });
        });

        addPanelLines([
          [-1.86, 0.13, 0],
          [1.6, 0.16, 0],
          [-1.32, 0.12, -0.28],
          [1.24, 0.15, -0.36],
          [-1.32, 0.12, 0.28],
          [1.24, 0.15, 0.36],
          [-0.92, 0.16, -0.46],
          [-0.92, 0.16, 0.46],
          [-0.38, 0.18, -0.58],
          [-0.38, 0.18, 0.58],
          [0.18, 0.2, -0.63],
          [0.18, 0.2, 0.63],
          [0.72, 0.21, -0.64],
          [0.72, 0.21, 0.64],
          [1.28, 0.14, -0.5],
          [1.28, 0.14, 0.5],
          [-0.64, 0.34, -0.18],
          [1, 0.34, -0.2],
          [-0.64, 0.34, 0.18],
          [1, 0.34, 0.2],
          [1.78, 0.14, -0.34],
          [1.78, 0.14, 0.34],
        ]);

        const baseGroup = new THREE.Group();
        baseGroup.position.z = -0.34;
        scene.add(baseGroup);
        [0.82, 1.16, 1.46].forEach((radius, index) => {
          const ring = new THREE.Mesh(
            new THREE.TorusGeometry(radius, 0.004, 8, 96),
            new THREE.MeshBasicMaterial({
              color: index === 1 ? 0x2ee8ff : 0xf8c76a,
              opacity: index === 1 ? 0.24 : 0.16,
              transparent: true,
            }),
          );
          ring.rotation.x = 0.28;
          baseGroup.add(ring);
        });

        const particleCount = 80;
        const positions = new Float32Array(particleCount * 3);
        for (let index = 0; index < particleCount; index += 1) {
          positions[index * 3] = (Math.random() - 0.5) * 3.4;
          positions[index * 3 + 1] = (Math.random() - 0.5) * 3.1;
          positions[index * 3 + 2] = (Math.random() - 0.5) * 1.8 - 0.3;
        }
        const particleGeometry = new THREE.BufferGeometry();
        particleGeometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
        const particleField = new THREE.Points(
          particleGeometry,
          new THREE.PointsMaterial({
            color: 0xa3fbff,
            opacity: 0.5,
            size: 0.018,
            transparent: true,
          }),
        );
        scene.add(particleField);

        const resize = () => {
          const width = hostElement.clientWidth || 140;
          const height = hostElement.clientHeight || 140;
          renderer.setSize(width, height, false);
          camera.aspect = width / height;
          camera.updateProjectionMatrix();
        };

        if (typeof ResizeObserver !== "undefined") {
          resizeObserver = new ResizeObserver(resize);
          resizeObserver.observe(hostElement);
        } else {
          window.addEventListener("resize", resize);
          removeWindowResize = () => window.removeEventListener("resize", resize);
        }
        resize();

        const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        const render = () => renderer.render(scene, camera);
        shipGroup.rotation.set(-0.34, -0.72, 0.06);

        if (!reduceMotion) {
          const animate = () => {
            const time = performance.now();
            shipGroup.rotation.x = -0.34 + Math.sin(time * 0.00052) * 0.045;
            shipGroup.rotation.y += 0.002;
            shipGroup.rotation.z = 0.06 + Math.sin(time * 0.00038) * 0.035;
            baseGroup.rotation.z -= 0.0025;
            particleField.rotation.z += 0.0008;
            render();
            frameId = window.requestAnimationFrame(animate);
          };
          animate();
        } else {
          shipGroup.rotation.set(-0.34, -0.72, 0.06);
          render();
        }

        if (!cancelled) {
          setWebglReady(true);
        }

        cleanupScene = () => {
          if (frameId) {
            window.cancelAnimationFrame(frameId);
          }
          resizeObserver?.disconnect();
          removeWindowResize?.();
          disposeObject(scene);
          renderer.dispose();
          renderer.domElement.remove();
        };
      } catch {
        if (!cancelled) {
          setWebglFailed(true);
        }
      }
    }

    void init();

    return () => {
      cancelled = true;
      cleanupScene?.();
    };
  }, []);

  return (
    <div
      aria-label={label}
      className={`vibe-skin-showcase-figure vibe-skin-space-ship ${
        webglReady ? "vibe-skin-space-ship-webgl-ready" : ""
      } ${webglFailed ? "vibe-skin-space-ship-webgl-failed" : ""} ${className}`}
      data-testid="vibe-skin-space-ship"
      role="img"
    >
      <span aria-hidden="true" className="vibe-skin-space-ship-webgl-host" ref={hostRef} />
      <span className="vibe-skin-space-ship-halo" />
      <span className="vibe-skin-space-ship-model vibe-skin-space-ship-fallback">
        <span className="vibe-skin-space-ship-shadow" />
        <span className="vibe-skin-space-ship-body" />
        <span className="vibe-skin-space-ship-nose" />
        <span className="vibe-skin-space-ship-spine" />
        <span className="vibe-skin-space-ship-wing vibe-skin-space-ship-wing-left" />
        <span className="vibe-skin-space-ship-wing vibe-skin-space-ship-wing-right" />
        <span className="vibe-skin-space-ship-core" />
        <span className="vibe-skin-space-ship-engine vibe-skin-space-ship-engine-left" />
        <span className="vibe-skin-space-ship-engine vibe-skin-space-ship-engine-right" />
        <span className="vibe-skin-space-ship-thruster vibe-skin-space-ship-thruster-left" />
        <span className="vibe-skin-space-ship-thruster vibe-skin-space-ship-thruster-right" />
      </span>
    </div>
  );
}
