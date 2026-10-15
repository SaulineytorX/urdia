"use strict";

(() => {
  const STORAGE_KEY = "urdia-launchboard-v1";
  const priorities = ["high", "medium", "low"];
  const statuses = ["backlog", "progress", "done"];
  const statusNames = { backlog: "Backlog", progress: "In progress", done: "Done" };
  const seeds = [
    { id: "discover", title: "Map the user journey", description: "Find the key moments from first visit to first value.", priority: "high", status: "backlog" },
    { id: "design", title: "Design the launch page", description: "Bring the story to life with a clear, compelling first impression.", priority: "high", status: "progress" },
    { id: "api", title: "Connect the workspace API", description: "Wire up the core endpoints and handle the edge cases.", priority: "medium", status: "backlog" },
    { id: "tests", title: "Run acceptance checks", description: "Walk through the critical flows. Make every detail count.", priority: "high", status: "progress" },
    { id: "docs", title: "Write the quickstart", description: "Help new users get from zero to their first small win.", priority: "low", status: "backlog" },
    { id: "release", title: "Prepare the release notes", description: "Tell the story of what’s new, improved, and ready to go.", priority: "low", status: "done" }
  ];
  const byId = (id) => document.getElementById(id);
  const search = byId("search");
  const priorityFilter = byId("priority-filter");
  const newTask = byId("new-task");
  const dialog = byId("task-dialog");
  const form = byId("task-form");
  const titleInput = byId("task-title");
  const descriptionInput = byId("task-description");
  const priorityInput = byId("task-priority");
  const titleError = byId("title-error");
  const formError = byId("form-error");
  const announcement = byId("announcements");
  let storageHealthy = true;
  let idCounter = 0;

  function seedTasks() {
    return seeds.map((task) => ({ ...task }));
  }

  function showStorageWarning(message) {
    storageHealthy = false;
    byId("saving-warning").textContent = message;
    byId("saving-warning").hidden = false;
    byId("save-status").textContent = "Changes are kept in this open tab";
  }

  function validTasks(value) {
    if (!Array.isArray(value) || value.length > 200) return false;
    const ids = new Set();
    return value.every((task) => {
      if (!task || typeof task !== "object" || Array.isArray(task)) return false;
      const keys = Object.keys(task);
      if (keys.length !== 5 || !["id", "title", "description", "priority", "status"].every((key) => Object.hasOwn(task, key))) return false;
      if (typeof task.id !== "string" || !/^[A-Za-z0-9_-]{1,80}$/.test(task.id) || ids.has(task.id)) return false;
      if (typeof task.title !== "string" || task.title.length < 1 || task.title.length > 80 || task.title.trim() !== task.title) return false;
      if (typeof task.description !== "string" || task.description.length > 280) return false;
      if (!priorities.includes(task.priority) || !statuses.includes(task.status)) return false;
      ids.add(task.id);
      return true;
    });
  }

  function loadTasks() {
    let raw;
    try {
      raw = localStorage.getItem(STORAGE_KEY);
    } catch {
      showStorageWarning("Browser storage is unavailable. You can still use the board and export your work; changes may be lost when this tab closes.");
      return seedTasks();
    }
    if (raw === null) return seedTasks();
    try {
      if (raw.length > 1000000) throw new Error("Storage exceeds limits");
      const parsed = JSON.parse(raw);
      if (!validTasks(parsed)) throw new Error("Invalid board data");
      return parsed.map(({ id, title, description, priority, status }) => ({ id, title, description, priority, status }));
    } catch {
      byId("saving-warning").textContent = "The saved board could not be read. The six starter tasks have been restored.";
      byId("saving-warning").hidden = false;
      return seedTasks();
    }
  }

  let tasks = loadTasks();

  function persist() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(tasks));
      if (!storageHealthy) {
        byId("saving-warning").hidden = true;
        storageHealthy = true;
      }
      byId("save-status").textContent = "Changes saved on this device";
    } catch {
      showStorageWarning("Your changes could not be saved to browser storage. Keep this tab open and export your board to keep a copy of your work.");
    }
  }

  function element(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function actionButton(action, text, label) {
    const button = element("button", "", text);
    button.type = "button";
    button.dataset.action = action;
    button.setAttribute("aria-label", label);
    button.title = label;
    return button;
  }

  function renderCard(task, index) {
    const card = element("article", "task-card");
    card.dataset.taskId = task.id;
    card.dataset.status = task.status;
    card.dataset.priority = task.priority;
    const top = element("div", "card-top");
    const priorityName = task.priority[0].toUpperCase() + task.priority.slice(1);
    top.append(element("span", `priority-badge ${task.priority}`, `${priorityName} priority`), element("span", "task-number", `LB-${String(index + 1).padStart(3, "0")}`));
    const title = element("h4", "", task.title);
    const description = element("p", "", task.description);
    const actions = element("div", "card-actions");
    const moves = element("div", "card-moves");
    const statusIndex = statuses.indexOf(task.status);
    const back = actionButton("back", "← Back", `Move ${task.title} to ${statusNames[statuses[statusIndex - 1]] || "previous lane"}`);
    back.disabled = statusIndex === 0;
    back.hidden = statusIndex === 0;
    const next = actionButton("next", statusIndex === 0 ? "Start task →" : "Complete ✓", `Move ${task.title} to ${statusNames[statuses[statusIndex + 1]] || "next lane"}`);
    next.disabled = statusIndex === 2;
    next.hidden = statusIndex === 2;
    moves.append(back, next);
    if (task.status === "done") moves.append(element("span", "completed-label mint", "✓ Completed"));
    const remove = actionButton("delete", "×", `Delete ${task.title}`);
    actions.append(moves, remove);
    card.append(top, title);
    if (task.description) card.append(description);
    card.append(actions);
    return card;
  }

  function visibleTasks() {
    const query = search.value.trim().toLowerCase();
    return tasks.filter((task) => (priorityFilter.value === "all" || task.priority === priorityFilter.value) && (!query || task.title.toLowerCase().includes(query) || task.description.toLowerCase().includes(query)));
  }

  function render() {
    const done = tasks.filter((task) => task.status === "done").length;
    const rate = tasks.length ? Math.round(done / tasks.length * 100) : 0;
    byId("total-count").textContent = String(tasks.length);
    byId("rail-count").textContent = String(tasks.length);
    byId("done-count").textContent = String(done);
    byId("completion-rate").textContent = `${rate}%`;
    byId("completion-progress").setAttribute("aria-valuenow", String(rate));
    byId("progress-fill").style.width = `${rate}%`;
    byId("progress-caption").textContent = tasks.length === 0 ? "A fresh start. Add your first task." : rate === 100 ? "Every step complete. Ready for liftoff." : "A strong start. Keep the momentum.";
    const filtered = visibleTasks();
    const hasFilters = search.value.trim() !== "" || priorityFilter.value !== "all";
    document.querySelectorAll("[data-lane]").forEach((lane) => {
      const laneTasks = filtered.filter((task) => task.status === lane.dataset.lane);
      lane.querySelector(".lane-count").textContent = String(laneTasks.length);
      const list = lane.querySelector(".task-list");
      list.replaceChildren();
      laneTasks.forEach((task) => list.append(renderCard(task, tasks.indexOf(task))));
      if (!laneTasks.length) {
        const message = hasFilters ? "No matching tasks here. Try another search or priority." : lane.dataset.lane === "backlog" ? "Room for your next idea. Add a new task to get started." : lane.dataset.lane === "progress" ? "Ready when you are. Start a task from the backlog." : "Your next win belongs here. Complete a task to see it.";
        list.append(element("div", "empty-lane", message));
      }
    });
  }

  function focusCardControl(taskId, preferredAction) {
    const card = Array.from(document.querySelectorAll(".task-card")).find((node) => node.dataset.taskId === taskId);
    if (!card) {
      newTask.focus();
      return;
    }
    const buttons = Array.from(card.querySelectorAll("button")).filter((button) => !button.hidden && !button.disabled);
    (buttons.find((button) => button.dataset.action === preferredAction) || buttons[0] || newTask).focus();
  }

  byId("board").addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button || button.disabled) return;
    const card = button.closest(".task-card");
    const task = tasks.find((item) => item.id === card.dataset.taskId);
    if (!task) return;
    const action = button.dataset.action;
    let focusId = task.id;
    if (action === "delete") {
      const visible = visibleTasks();
      const index = visible.findIndex((item) => item.id === task.id);
      focusId = (visible[index + 1] || visible[index - 1] || {}).id;
      tasks = tasks.filter((item) => item.id !== task.id);
      announcement.textContent = `Deleted ${task.title}.`;
    } else {
      const nextIndex = statuses.indexOf(task.status) + (action === "next" ? 1 : -1);
      if (nextIndex < 0 || nextIndex >= statuses.length) return;
      task.status = statuses[nextIndex];
      announcement.textContent = `${task.title} moved to ${statusNames[task.status]}.`;
    }
    persist();
    render();
    focusCardControl(focusId, action);
  });

  search.addEventListener("input", render);
  priorityFilter.addEventListener("change", render);

  function resetForm() {
    form.reset();
    titleError.hidden = true;
    titleError.textContent = "";
    formError.hidden = true;
    formError.textContent = "";
    titleInput.removeAttribute("aria-invalid");
  }

  newTask.addEventListener("click", () => {
    resetForm();
    dialog.showModal();
    titleInput.focus();
  });
  byId("cancel-task").addEventListener("click", () => dialog.close());
  dialog.addEventListener("close", () => {
    resetForm();
    newTask.focus();
  });
  titleInput.addEventListener("input", () => {
    if (titleInput.value.trim()) {
      titleError.hidden = true;
      titleInput.removeAttribute("aria-invalid");
    }
  });

  function uniqueId() {
    let id;
    do {
      idCounter += 1;
      id = `task-${Date.now().toString(36)}-${idCounter.toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
    } while (tasks.some((task) => task.id === id));
    return id;
  }

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const title = titleInput.value.trim();
    if (!title || title.length > 80) {
      titleError.textContent = !title ? "Please enter a task title. Spaces alone don’t count." : "Keep the title to 80 characters or fewer.";
      titleError.hidden = false;
      titleInput.setAttribute("aria-invalid", "true");
      titleInput.focus();
      return;
    }
    if (descriptionInput.value.length > 280 || !priorities.includes(priorityInput.value)) {
      formError.textContent = "Use a description of 280 characters or fewer and choose a valid priority.";
      formError.hidden = false;
      return;
    }
    if (tasks.length >= 200) {
      formError.textContent = "This board holds up to 200 tasks. Delete a task before adding another.";
      formError.hidden = false;
      return;
    }
    tasks.push({ id: uniqueId(), title, description: descriptionInput.value, priority: priorityInput.value, status: "backlog" });
    persist();
    render();
    dialog.close();
    newTask.focus();
    announcement.textContent = `Added ${title} to Backlog${visibleTasks().some((task) => task.title === title) ? "." : ". Clear your filters to see it."}`;
  });

  byId("export-board").addEventListener("click", () => {
    const blob = new Blob([JSON.stringify(tasks, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "launchboard.json";
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    announcement.textContent = "Board exported as launchboard.json.";
  });

  byId("reset-board").addEventListener("click", () => {
    if (!window.confirm("Restore the six starter tasks? This replaces all tasks on this board and clears your filters.")) return;
    tasks = seedTasks();
    search.value = "";
    priorityFilter.value = "all";
    persist();
    render();
    announcement.textContent = "Board reset to the six starter tasks. Filters cleared.";
  });

  render();
  if (storageHealthy) persist();
})();
