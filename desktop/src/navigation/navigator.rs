use super::Route;

#[derive(Debug, Clone, Default)]
pub struct Navigator {
    current: Route,
    history: Vec<Route>,
}

impl Navigator {
    pub fn current(&self) -> Route {
        self.current
    }

    pub fn navigate(&mut self, route: Route) {
        if route != self.current {
            self.history.push(self.current);
            self.current = route;
        }
    }

    pub fn replace(&mut self, route: Route) {
        self.current = route;
    }

    pub fn back(&mut self) -> bool {
        let Some(previous) = self.history.pop() else {
            return false;
        };

        self.current = previous;
        true
    }
}
