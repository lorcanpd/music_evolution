(function () {
  const state = {
    metadata: null,
    activeSpotIndex: null,
    activeTree: null,
    activePathTarget: null,
    loadingTree: false,
    audio: new Audio(),
    currentSongId: null,
    currentButton: null,
    treeCache: new Map(),
    hoverTooltip: null,
  };

  document.addEventListener("DOMContentLoaded", init);

  async function init() {
    const app = document.getElementById("family-tree-app");
    if (!app) return;

    try {
      const metadata = await fetchJson("/api/family_trees");
      if (!metadata || !metadata.population) {
        renderRebuildingState(app, metadata?.message || "Family trees are being rebuilt.");
        pollForMetadata(app);
        return;
      }

      state.metadata = metadata;
      state.hoverTooltip = createTooltip();
      renderApp();
    } catch (error) {
      renderRebuildingState(app, `Failed to load family trees: ${error.message}`);
      pollForMetadata(app);
    }
  }

  async function pollForMetadata(app) {
    setTimeout(async () => {
      try {
        const metadata = await fetchJson("/api/family_trees");
        if (metadata && metadata.population) {
          state.metadata = metadata;
          state.hoverTooltip = state.hoverTooltip || createTooltip();
          renderApp();
        } else {
          pollForMetadata(app);
        }
      } catch (_) {
        pollForMetadata(app);
      }
    }, 5000);
  }

  function renderRebuildingState(app, message) {
    app.innerHTML = `
      <article class="card family-tree-status">
        <h2>Family Trees</h2>
        <div class="spinner"></div>
        <p>${escapeHtml(message)}</p>
      </article>
    `;
  }

  function renderApp() {
    const app = document.getElementById("family-tree-app");
    if (!app || !state.metadata) return;

    app.innerHTML = `
      <section class="family-tree-stage">
        <div id="family-tree-panel" class="family-tree-panel"></div>
        <div id="family-tree-population" class="family-tree-population"></div>
      </section>
    `;

    renderTreePanel();
    renderPopulation();
  }

  function renderTreePanel() {
    const panel = document.getElementById("family-tree-panel");
    if (!panel || !state.metadata) return;

    if (state.loadingTree) {
      panel.innerHTML = `
        <article class="card family-tree-status">
          <div class="spinner"></div>
          <p>Loading spotlight tree...</p>
        </article>
      `;
      return;
    }

    if (!state.activeTree || state.activeSpotIndex === null) {
      const spotlightMarkup = state.metadata.spotlights.map((spotlight) => `
        <button type="button" class="family-tree-spotlight-chip" data-activate-placeholder="${spotlight.index}" style="--island-accent:${islandColor(spotlight.node)}">
          <span>Spotlight ${spotlight.index + 1}</span>
          <strong>#${spotlight.song_id}</strong>
          <em>Island ${spotlight.node}</em>
        </button>
      `).join("");

      panel.innerHTML = `
        <article class="card family-tree-placeholder">
          <div class="family-tree-placeholder-copy">
            <h2>Previous Generation #${state.metadata.previous_generation}</h2>
            <p>Pick one of the three spotlight songs to reveal a compact pedigree view above the population.</p>
          </div>
          <div class="family-tree-spotlight-row">${spotlightMarkup}</div>
        </article>
      `;

      panel.querySelectorAll("[data-activate-placeholder]").forEach((button) => {
        button.addEventListener("click", async () => {
          await toggleSpotlight(Number(button.dataset.activatePlaceholder));
        });
      });
      return;
    }

    const tree = state.activeTree;
    const rowOrder = [
      state.metadata.previous_generation - 2,
      state.metadata.previous_generation - 1,
      state.metadata.previous_generation,
    ].filter((generation, index, array) => generation >= 0 && array.indexOf(generation) === index);

    const rowHeights = 170;
    const height = rowOrder.length * rowHeights;
    const yByGeneration = new Map(rowOrder.map((generation, index) => [generation, 72 + index * rowHeights]));
    const edgeMap = new Map(tree.edges.map((edge) => [edgeKey(edge.parent_song_id, edge.child_song_id), edge]));
    const ancestryEdgeKeys = computeAncestryEdgeKeys(tree);
    const revealedEdgeKeys = state.activePathTarget === null
      ? ancestryEdgeKeys
      : computeRevealedEdgeKeys(tree, edgeMap, state.activePathTarget) || ancestryEdgeKeys;
    const activePlayableSet = new Set(tree.nodes.map((node) => node.song_id));

    const svgEdges = tree.edges.map((edge) => {
      const parent = tree.nodes.find((node) => node.song_id === edge.parent_song_id);
      const child = tree.nodes.find((node) => node.song_id === edge.child_song_id);
      if (!parent || !child) return "";

      const active = revealedEdgeKeys.has(edgeKey(edge.parent_song_id, edge.child_song_id));
      const x1 = 80 + parent.x * 840;
      const y1 = (yByGeneration.get(parent.generation) || 0) + 48;
      const x2 = 80 + child.x * 840;
      const y2 = (yByGeneration.get(child.generation) || 0) - 10;
      const weight = Math.max(1.5, edge.similarity * 8.5);
      const opacity = active ? Math.max(0.45, edge.similarity) : 0.12;
      const cls = active ? "tree-edge tree-edge-active" : "tree-edge";

      return `<line class="${cls}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" style="stroke-width:${weight}px;opacity:${opacity}"></line>`;
    }).join("");

    const rowLabels = rowOrder.map((generation, index) => {
      const rowName = rowOrder.length - index === 3 ? "G-3" : rowOrder.length - index === 2 ? "G-2" : "G-1";
      return `<div class="family-tree-row-label" style="top:${(yByGeneration.get(generation) || 0) - 38}px">${rowName}</div>`;
    }).join("");

    const nodeMarkup = tree.nodes.map((node) => {
      const color = islandColor(node.node);
      const y = yByGeneration.get(node.generation) || 0;
      const isSpotlight = node.song_id === tree.spotlight_song_id;
      const isPathTarget = node.song_id === state.activePathTarget;
      const hasRevealPath = tree.reveal_paths.some((path) => path.target_song_id === node.song_id);
      const interactive = hasRevealPath || isSpotlight;
      const buttonClass = [
        "tree-node",
        `tree-role-${node.role.replace(/_/g, "-")}`,
        isSpotlight ? "is-spotlight" : "",
        isPathTarget ? "is-path-target" : "",
        interactive ? "is-interactive" : "",
      ].filter(Boolean).join(" ");

      return `
        <button
          type="button"
          class="${buttonClass}"
          data-song-id="${node.song_id}"
          data-node-role="${node.role}"
          style="left:${6 + node.x * 88}%; top:${y}px; --island-accent:${color};"
        >
          <span class="tree-node-label">Island ${node.node}</span>
          <span class="tree-node-id">#${node.song_id}</span>
          <span class="tree-node-role">${formatRole(node.role)}</span>
          <span class="tree-node-actions">
            <span class="tree-node-similarity">${Math.round(node.spotlight_similarity * 100)}%</span>
            <span class="tree-node-play" data-play-song="${node.song_id}">Play</span>
          </span>
        </button>
      `;
    }).join("");

    panel.innerHTML = `
      <article class="card family-tree-card">
        <div class="family-tree-header">
          <div>
            <h2>Previous Generation #${state.metadata.previous_generation}</h2>
            <p class="meta">Spotlight song #${tree.spotlight_song_id}</p>
          </div>
          <div class="family-tree-header-actions">
            <button type="button" class="btn btn-secondary" id="collapse-family-tree">Collapse</button>
          </div>
        </div>
        <div id="family-tree-canvas" class="family-tree-canvas" style="height:${height}px">
          ${rowLabels}
          <svg class="family-tree-svg" viewBox="0 0 1000 ${height}" preserveAspectRatio="none">
            ${svgEdges}
          </svg>
          ${nodeMarkup}
        </div>
      </article>
    `;

    document.getElementById("collapse-family-tree")?.addEventListener("click", () => {
      state.activeSpotIndex = null;
      state.activeTree = null;
      state.activePathTarget = null;
      renderApp();
    });

    const canvas = document.getElementById("family-tree-canvas");
    canvas?.addEventListener("click", (event) => {
      if (event.target === canvas || event.target.classList.contains("family-tree-svg")) {
        state.activePathTarget = null;
        renderTreePanel();
      }
    });

    panel.querySelectorAll(".tree-node").forEach((button) => {
      button.addEventListener("click", (event) => {
        const target = event.target instanceof Element ? event.target : null;
        const playHandle = target ? target.closest("[data-play-song]") : null;
        const songId = Number(button.dataset.songId);
        if (playHandle) {
          event.preventDefault();
          event.stopPropagation();
          togglePlayback(songId, `/family_tree_wav/${songId}`, playHandle);
          return;
        }

        if (songId === tree.spotlight_song_id) {
          state.activePathTarget = null;
        } else if (tree.reveal_paths.some((path) => path.target_song_id === songId)) {
          state.activePathTarget = songId;
        }
        renderTreePanel();
      });

      button.addEventListener("mouseenter", (event) => {
        const songId = Number(button.dataset.songId);
        const node = tree.nodes.find((candidate) => candidate.song_id === songId);
        if (!node) return;
        showTooltip(event, node);
      });
      button.addEventListener("mousemove", moveTooltip);
      button.addEventListener("mouseleave", hideTooltip);
    });

    state.activePlayableSet = activePlayableSet;
  }

  function renderPopulation() {
    const container = document.getElementById("family-tree-population");
    if (!container || !state.metadata) return;

    const activeRelativeIds = new Set(
      (state.activeTree?.nodes || [])
        .filter((node) => node.generation === state.metadata.previous_generation)
        .map((node) => node.song_id)
    );

    const grouped = new Map();
    state.metadata.population.forEach((song) => {
      if (!grouped.has(song.node)) grouped.set(song.node, []);
      grouped.get(song.node).push(song);
    });

    const groupsMarkup = [...grouped.entries()].map(([node, songs]) => {
      const cards = songs.map((song) => {
      const spotlightSummary = state.metadata.spotlights.find((spot) => spot.song_id === song.song_id);
      const isActiveSpotlight = spotlightSummary && spotlightSummary.index === state.activeSpotIndex;
      const isRelative = !spotlightSummary && activeRelativeIds.has(song.song_id);
      const playable = Boolean(spotlightSummary) || isRelative;
      const buttonLabel = spotlightSummary
        ? (isActiveSpotlight ? "Hide Tree" : "Show Tree")
        : "Relative";
      const cardClasses = [
        "population-card",
        spotlightSummary ? "is-spotlight" : "",
        isActiveSpotlight ? "is-active" : "",
        isRelative ? "is-relative" : "",
        playable ? "is-playable" : "is-inert",
      ].filter(Boolean).join(" ");

      return `
        <article class="${cardClasses}" style="--island-accent:${islandColor(song.node)}" ${spotlightSummary ? `data-card-spot="${spotlightSummary.index}"` : ""}>
          <div class="population-card-meta">
            <span class="population-card-island">Island ${song.node}</span>
            ${spotlightSummary ? `<span class="population-card-badge">Spotlight ${spotlightSummary.index + 1}</span>` : ""}
          </div>
          <h3>#${song.song_id}</h3>
          <p>${spotlightSummary ? "Always playable" : (isRelative ? "Playable for this spotlight" : "Locked until a related spotlight is active")}</p>
          ${spotlightSummary ? `
            <div class="population-card-actions">
              <button type="button" class="btn btn-secondary" data-play-song="${song.song_id}">Play</button>
              <button type="button" class="btn btn-primary" data-activate-spot="${spotlightSummary.index}">${buttonLabel}</button>
            </div>
          ` : isRelative ? `
            <div class="population-card-actions">
              <button type="button" class="btn btn-secondary" data-play-song="${song.song_id}">Play</button>
              <span class="population-card-status">Relative</span>
            </div>
          ` : `
            <div class="population-card-actions">
              <span class="population-card-status is-muted">Locked</span>
            </div>
          `}
        </article>
      `;
      }).join("");

      return `
        <section class="population-island-group" style="--island-accent:${islandColor(node)}">
          <div class="population-island-header">
            <div>
              <h3>Island ${node}</h3>
              <p>${songs.length} songs</p>
            </div>
          </div>
          <div class="population-island-grid">${cards}</div>
        </section>
      `;
    }).join("");

    container.innerHTML = `
      <article class="card family-tree-population-card">
        <div class="family-tree-header">
          <div>
            <h2>Previous Generation #${state.metadata.previous_generation}</h2>
            <p class="meta">Population grouped by island. Spotlight songs are always playable.</p>
          </div>
        </div>
        <div class="population-groups">${groupsMarkup}</div>
      </article>
    `;

    container.querySelectorAll("[data-play-song]").forEach((button) => {
      button.addEventListener("click", (event) => {
        const songId = Number(button.dataset.playSong);
        togglePlayback(songId, `/family_tree_wav/${songId}`, button);
        event.stopPropagation();
      });
    });

    container.querySelectorAll("[data-activate-spot]").forEach((button) => {
      if (!button.dataset.activateSpot) return;
      button.addEventListener("click", async () => {
        const spotIndex = Number(button.dataset.activateSpot);
        await toggleSpotlight(spotIndex);
      });
    });

    container.querySelectorAll("[data-card-spot]").forEach((card) => {
      card.addEventListener("click", async (event) => {
        const target = event.target instanceof Element ? event.target : null;
        if (target && (target.closest("[data-play-song]") || target.closest("[data-activate-spot]"))) {
          return;
        }
        await toggleSpotlight(Number(card.dataset.cardSpot));
      });
    });
  }

  async function activateSpotlight(spotIndex) {
    state.loadingTree = true;
    state.activeSpotIndex = spotIndex;
    state.activePathTarget = null;
    renderTreePanel();
    renderPopulation();

    try {
      const cached = state.treeCache.get(spotIndex);
      const tree = cached || await fetchJson(`/api/family_trees/${spotIndex}`);
      if (tree.status && !tree.nodes) {
        throw new Error(tree.message || tree.status);
      }
      state.treeCache.set(spotIndex, tree);
      state.activeTree = tree;
    } catch (error) {
      state.activeTree = null;
      state.activeSpotIndex = null;
      const panel = document.getElementById("family-tree-panel");
      if (panel) {
        panel.innerHTML = `
          <article class="card family-tree-status">
            <h2>Family Trees</h2>
            <p>Failed to load spotlight tree: ${escapeHtml(error.message)}</p>
          </article>
        `;
      }
    } finally {
      state.loadingTree = false;
      renderTreePanel();
      renderPopulation();
    }
  }

  async function toggleSpotlight(spotIndex) {
    if (state.activeSpotIndex === spotIndex) {
      state.activeSpotIndex = null;
      state.activeTree = null;
      state.activePathTarget = null;
      renderApp();
      return;
    }

    await activateSpotlight(spotIndex);
  }

  async function togglePlayback(songId, url, buttonLike) {
    if (!buttonLike) return;

    if (state.currentSongId === songId && !state.audio.paused) {
      state.audio.pause();
      resetCurrentButton();
      return;
    }

    resetCurrentButton();
    state.currentSongId = songId;
    state.currentButton = buttonLike;
    buttonLike.textContent = "Pause";

    if (state.audio.src !== new URL(url, window.location.origin).toString()) {
      state.audio.src = url;
    }

    try {
      await state.audio.play();
    } catch (_) {
      resetCurrentButton();
    }
  }

  state.audio.addEventListener("ended", resetCurrentButton);
  state.audio.addEventListener("pause", () => {
    if (state.audio.ended) return;
    if (state.currentButton && state.audio.currentTime > 0) {
      state.currentButton.textContent = "Play";
    }
  });

  function resetCurrentButton() {
    if (state.currentButton) {
      state.currentButton.textContent = "Play";
    }
    state.currentButton = null;
    state.currentSongId = null;
  }

  function computeAncestryEdgeKeys(tree) {
    const parentByChild = new Map();
    tree.edges.forEach((edge) => {
      const list = parentByChild.get(edge.child_song_id) || [];
      list.push(edge.parent_song_id);
      parentByChild.set(edge.child_song_id, list);
    });

    const active = new Set();
    const queue = [tree.spotlight_song_id];
    const seen = new Set(queue);

    while (queue.length > 0) {
      const childId = queue.shift();
      const parents = parentByChild.get(childId) || [];
      parents.forEach((parentId) => {
        active.add(edgeKey(parentId, childId));
        if (!seen.has(parentId)) {
          seen.add(parentId);
          queue.push(parentId);
        }
      });
    }

    return active;
  }

  function computeRevealedEdgeKeys(tree, edgeMap, targetSongId) {
    const revealPath = tree.reveal_paths.find((path) => path.target_song_id === targetSongId);
    if (!revealPath) return null;

    const keys = new Set();
    for (let index = 0; index < revealPath.node_path.length - 1; index += 1) {
      const a = revealPath.node_path[index];
      const b = revealPath.node_path[index + 1];
      const forward = edgeKey(a, b);
      const reverse = edgeKey(b, a);
      if (edgeMap.has(forward)) {
        keys.add(forward);
      } else if (edgeMap.has(reverse)) {
        keys.add(reverse);
      }
    }
    return keys;
  }

  function createTooltip() {
    const tooltip = document.createElement("div");
    tooltip.className = "family-tree-tooltip";
    tooltip.hidden = true;
    document.body.appendChild(tooltip);
    return tooltip;
  }

  function showTooltip(event, node) {
    if (!state.hoverTooltip) return;
    const percent = Math.round(node.spotlight_similarity * 100);
    state.hoverTooltip.innerHTML = `
      <div class="family-tree-tooltip-ring" style="--ring-fill:${percent}%"></div>
      <div class="family-tree-tooltip-copy">
        <strong>Song #${node.song_id}</strong>
        <span>${formatRole(node.role)}</span>
        <span>${percent}% related to spotlight</span>
      </div>
    `;
    state.hoverTooltip.hidden = false;
    moveTooltip(event);
  }

  function moveTooltip(event) {
    if (!state.hoverTooltip || state.hoverTooltip.hidden) return;
    state.hoverTooltip.style.left = `${event.pageX + 18}px`;
    state.hoverTooltip.style.top = `${event.pageY + 18}px`;
  }

  function hideTooltip() {
    if (state.hoverTooltip) {
      state.hoverTooltip.hidden = true;
    }
  }

  async function fetchJson(url) {
    const response = await fetch(url, { headers: { Accept: "application/json" } });
    const text = await response.text();
    return text ? JSON.parse(text) : null;
  }

  function islandColor(node) {
    const palette = state.metadata?.islands?.find((island) => island.node === node);
    return palette ? palette.color : "#ffffff";
  }

  function formatRole(role) {
    return role.replace(/_/g, " ");
  }

  function edgeKey(parentId, childId) {
    return `${parentId}:${childId}`;
  }

  function escapeHtml(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }
})();
