use super::Route;

#[derive(Debug, Clone)]
pub struct Navigator {
    current: Route,
    history: Vec<Route>,
}

impl Default for Navigator {
    fn default() -> Self {
        Self {
            current: Route::default(),
            history: Vec::new(),
        }
    }
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
