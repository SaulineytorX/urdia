def dependency_order(graph):
    if not isinstance(graph, dict):
        raise TypeError("graph must be a dict")

    for node, prerequisites in graph.items():
        if not isinstance(node, str):
            raise TypeError("node names must be strings")
        if not node:
            raise ValueError("node names must not be empty")
        if not isinstance(prerequisites, (list, tuple)):
            raise TypeError("prerequisites must be lists or tuples")
        for prerequisite in prerequisites:
            if not isinstance(prerequisite, str):
                raise TypeError("prerequisite names must be strings")
            if not prerequisite:
                raise ValueError("prerequisite names must not be empty")

    remaining = {}
    dependents = {node: [] for node in graph}
    for node, prerequisites in graph.items():
        unique = set(prerequisites)
        for prerequisite in unique:
            if prerequisite not in graph:
                raise ValueError("unknown prerequisite")
            if prerequisite == node:
                raise ValueError("self-dependency")
            dependents[prerequisite].append(node)
        remaining[node] = len(unique)

    ready = {node for node in graph if remaining[node] == 0}
    order = []
    while ready:
        node = min(ready)
        ready.remove(node)
        order.append(node)
        for dependent in dependents[node]:
            remaining[dependent] -= 1
            if remaining[dependent] == 0:
                ready.add(dependent)

    if len(order) != len(graph):
        raise ValueError("dependency cycle")
    return order
