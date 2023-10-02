fn start_nobreak(&mut self) {
        let (s, annotation) = self.decorator.decorate_code_start();
        self.ann_stack.push(annotation);
        self.add_inline_text(&s);
    }