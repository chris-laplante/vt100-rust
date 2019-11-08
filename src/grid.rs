use std::convert::TryInto as _;

#[derive(Clone, Debug)]
pub struct Grid {
    size: Size,
    pos: Pos,
    saved_pos: Pos,
    rows: Vec<crate::row::Row>,
    scroll_top: u16,
    scroll_bottom: u16,
    origin_mode: bool,
    saved_origin_mode: bool,
}

impl Grid {
    pub fn new(size: Size) -> Self {
        Self {
            size,
            pos: Pos::default(),
            saved_pos: Pos::default(),
            rows: vec![crate::row::Row::new(size.cols); size.rows as usize],
            scroll_top: 0,
            scroll_bottom: size.rows - 1,
            origin_mode: false,
            saved_origin_mode: false,
        }
    }

    fn new_row(&self) -> crate::row::Row {
        crate::row::Row::new(self.size.cols)
    }

    pub fn clear(&mut self) {
        self.pos = Pos::default();
        self.saved_pos = Pos::default();
        for row in self.rows_mut() {
            row.clear(crate::attrs::Color::Default);
        }
        self.scroll_top = 0;
        self.scroll_bottom = self.size.rows - 1;
        self.origin_mode = false;
        self.saved_origin_mode = false;
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn set_size(&mut self, size: Size) {
        if size.cols != self.size.cols {
            for row in &mut self.rows {
                row.wrap(false);
            }
        }

        self.size = size;

        if self.scroll_bottom >= size.rows {
            self.scroll_bottom = size.rows - 1;
        }
        if self.scroll_bottom < self.scroll_top {
            self.scroll_top = 0;
        }

        self.row_clamp_top(false);
        self.row_clamp_bottom(false);
        self.col_clamp();
    }

    pub fn pos(&self) -> Pos {
        self.pos
    }

    pub fn set_pos(&mut self, mut pos: Pos) {
        if self.origin_mode {
            pos.row = pos.row.saturating_add(self.scroll_top);
        }
        self.pos = pos;
        self.row_clamp_top(self.origin_mode);
        self.row_clamp_bottom(self.origin_mode);
        self.col_clamp();
    }

    pub fn save_cursor(&mut self) {
        self.saved_pos = self.pos;
        self.saved_origin_mode = self.origin_mode;
    }

    pub fn restore_cursor(&mut self) {
        self.pos = self.saved_pos;
        self.origin_mode = self.saved_origin_mode;
    }

    pub fn rows(&self) -> impl Iterator<Item = &crate::row::Row> {
        self.rows.iter()
    }

    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut crate::row::Row> {
        self.rows.iter_mut()
    }

    pub fn row(&self, pos: Pos) -> Option<&crate::row::Row> {
        self.rows.get(pos.row as usize)
    }

    pub fn row_mut(&mut self, pos: Pos) -> Option<&mut crate::row::Row> {
        self.rows.get_mut(pos.row as usize)
    }

    pub fn current_row_mut(&mut self) -> &mut crate::row::Row {
        self.row_mut(self.pos)
            .expect("cursor not pointing to a cell")
    }

    pub fn cell(&self, pos: Pos) -> Option<&crate::cell::Cell> {
        self.row(pos).and_then(|r| r.get(pos.col))
    }

    pub fn cell_mut(&mut self, pos: Pos) -> Option<&mut crate::cell::Cell> {
        self.row_mut(pos).and_then(|r| r.get_mut(pos.col))
    }

    pub fn current_cell_mut(&mut self) -> &mut crate::cell::Cell {
        self.cell_mut(self.pos)
            .expect("cursor not pointing to a cell")
    }

    pub fn contents(&self) -> String {
        let mut contents = String::new();
        for row in self.rows() {
            contents += &row.contents(0, self.size.cols);
            if !row.wrapped() {
                contents += "\n";
            }
        }
        contents.trim_end().to_string()
    }

    pub fn contents_formatted(&self) -> Vec<u8> {
        let mut contents = b"\x1b[H\x1b[J".to_vec();
        let mut prev_attrs = crate::attrs::Attrs::default();
        let mut final_col = 0;
        for row in self.rows() {
            let (mut new_contents, new_attrs, new_col) =
                row.contents_formatted(0, self.size.cols, prev_attrs);
            if !new_contents.is_empty() {
                final_col = new_col;
            }
            contents.append(&mut new_contents);
            if !row.wrapped() {
                contents.extend(b"\r\n");
            }
            prev_attrs = new_attrs;
        }

        let mut final_row = self.size.rows;
        while contents.ends_with(b"\r\n") {
            contents.truncate(contents.len() - 2);
            final_row -= 1;
        }

        if final_row != self.pos.row || final_col != self.pos.col {
            contents.extend(
                format!("\x1b[{};{}H", self.pos.row + 1, self.pos.col + 1)
                    .as_bytes(),
            );
        }

        contents
    }

    pub fn contents_diff(&self, prev: &Self) -> Vec<u8> {
        let mut contents = b"\x1b[m".to_vec();
        let mut prev_attrs = crate::attrs::Attrs::default();
        let mut final_row = prev.pos.row;
        let mut final_col = prev.pos.col;
        for (idx, (row, prev_row)) in self.rows().zip(prev.rows()).enumerate()
        {
            let (mut new_contents, new_attrs, new_col) =
                row.contents_diff(prev_row, 0, self.size.cols, prev_attrs);
            if !new_contents.is_empty() {
                contents.extend(format!("\x1b[{};1H", idx + 1).as_bytes());
                final_row = idx.try_into().unwrap();
                final_col = new_col;
            }
            contents.append(&mut new_contents);
            prev_attrs = new_attrs;
        }

        if self.pos.row != final_row || self.pos.col != final_col {
            contents.extend(
                format!("\x1b[{};{}H", self.pos.row + 1, self.pos.col + 1)
                    .as_bytes(),
            );
        }

        contents
    }

    pub fn erase_all(&mut self, bgcolor: crate::attrs::Color) {
        for row in self.rows_mut() {
            row.clear(bgcolor);
        }
    }

    pub fn erase_all_forward(&mut self, bgcolor: crate::attrs::Color) {
        let pos = self.pos;
        for row in self.rows_mut().skip(pos.row as usize + 1) {
            row.clear(bgcolor);
        }

        self.erase_row_forward(bgcolor);
    }

    pub fn erase_all_backward(&mut self, bgcolor: crate::attrs::Color) {
        let pos = self.pos;
        for row in self.rows_mut().take(pos.row as usize) {
            row.clear(bgcolor);
        }

        self.erase_row_backward(bgcolor);
    }

    pub fn erase_row(&mut self, bgcolor: crate::attrs::Color) {
        self.current_row_mut().clear(bgcolor);
    }

    pub fn erase_row_forward(&mut self, bgcolor: crate::attrs::Color) {
        let pos = self.pos;
        let row = self.current_row_mut();
        row.wrap(false);
        for cell in row.cells_mut().skip(pos.col as usize) {
            cell.clear(bgcolor);
        }
    }

    pub fn erase_row_backward(&mut self, bgcolor: crate::attrs::Color) {
        let pos = self.pos;
        let row = self.current_row_mut();
        for cell in row.cells_mut().take(pos.col as usize + 1) {
            cell.clear(bgcolor);
        }
    }

    pub fn insert_cells(&mut self, count: u16) {
        let size = self.size;
        let pos = self.pos;
        let row = self.current_row_mut();
        for _ in 0..count {
            row.insert(pos.col as usize, crate::cell::Cell::default());
        }
        row.truncate(size.cols as usize);
    }

    pub fn delete_cells(&mut self, count: u16) {
        let size = self.size;
        let pos = self.pos;
        let row = self.current_row_mut();
        for _ in 0..(count.min(size.cols - pos.col)) {
            row.remove(pos.col as usize);
        }
        row.resize(size.cols as usize, crate::cell::Cell::default());
    }

    pub fn erase_cells(&mut self, count: u16, bgcolor: crate::attrs::Color) {
        let pos = self.pos;
        let row = self.current_row_mut();
        for cell in
            row.cells_mut().skip(pos.col as usize).take(count as usize)
        {
            cell.clear(bgcolor);
        }
    }

    pub fn insert_lines(&mut self, count: u16) {
        for _ in 0..count {
            self.rows.remove(self.scroll_bottom as usize);
            self.rows.insert(self.pos.row as usize, self.new_row());
        }
    }

    pub fn delete_lines(&mut self, count: u16) {
        for _ in 0..(count.min(self.size.rows - self.pos.row)) {
            self.rows
                .insert(self.scroll_bottom as usize + 1, self.new_row());
            self.rows.remove(self.pos.row as usize);
        }
    }

    pub fn scroll_up(&mut self, count: u16) {
        for _ in 0..(count.min(self.size.rows - self.scroll_top)) {
            self.rows
                .insert(self.scroll_bottom as usize + 1, self.new_row());
            self.rows.remove(self.scroll_top as usize);
        }
    }

    pub fn scroll_down(&mut self, count: u16) {
        for _ in 0..count {
            self.rows.remove(self.scroll_bottom as usize);
            self.rows.insert(self.scroll_top as usize, self.new_row());
        }
    }

    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let bottom = bottom.min(self.size().rows - 1);
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        } else {
            self.scroll_top = 0;
            self.scroll_bottom = self.size().rows - 1;
        }
        self.pos.row = self.scroll_top;
        self.pos.col = 0;
    }

    pub fn set_origin_mode(&mut self, mode: bool) {
        self.origin_mode = mode;
        self.set_pos(Pos { row: 0, col: 0 })
    }

    pub fn row_inc_clamp(&mut self, count: u16) {
        self.pos.row = self.pos.row.saturating_add(count);
        self.row_clamp_bottom(true);
    }

    pub fn row_inc_scroll(&mut self, count: u16) {
        self.pos.row = self.pos.row.saturating_add(count);
        let lines = self.row_clamp_bottom(true);
        self.scroll_up(lines);
    }

    pub fn row_dec_clamp(&mut self, count: u16) {
        self.pos.row = self.pos.row.saturating_sub(count);
        self.row_clamp_top(true);
    }

    pub fn row_dec_scroll(&mut self, count: u16) {
        // need to account for clamping by both row_clamp_top and by
        // saturating_sub
        let extra_lines = if count > self.pos.row {
            count - self.pos.row
        } else {
            0
        };
        self.pos.row = self.pos.row.saturating_sub(count);
        let lines = self.row_clamp_top(true);
        self.scroll_down(lines + extra_lines);
    }

    pub fn row_set(&mut self, i: u16) {
        self.pos.row = i;
        self.row_clamp_top(true);
        self.row_clamp_bottom(true);
    }

    pub fn col_inc(&mut self, count: u16) {
        self.pos.col = self.pos.col.saturating_add(count);
    }

    pub fn col_inc_clamp(&mut self, count: u16) {
        self.pos.col = self.pos.col.saturating_add(count);
        self.col_clamp();
    }

    pub fn col_dec(&mut self, count: u16) {
        self.pos.col = self.pos.col.saturating_sub(count);
    }

    pub fn col_tab(&mut self) {
        self.pos.col -= self.pos.col % 8;
        self.pos.col += 8;
        self.col_clamp();
    }

    pub fn col_set(&mut self, i: u16) {
        self.pos.col = i;
        self.col_clamp();
    }

    pub fn col_wrap(&mut self, width: u16) {
        if self.pos.col > self.size.cols - width {
            self.current_row_mut().wrap(true);
            self.pos.col = 0;
            self.row_inc_scroll(1);
        }
    }

    fn row_clamp_top(&mut self, limit_to_scroll_region: bool) -> u16 {
        if limit_to_scroll_region && self.pos.row < self.scroll_top {
            let rows = self.scroll_top - self.pos.row;
            self.pos.row = self.scroll_top;
            rows
        } else {
            0
        }
    }

    fn row_clamp_bottom(&mut self, limit_to_scroll_region: bool) -> u16 {
        let bottom = if limit_to_scroll_region {
            self.scroll_bottom
        } else {
            self.size.rows - 1
        };
        if self.pos.row > bottom {
            let rows = self.pos.row - bottom;
            self.pos.row = bottom;
            rows
        } else {
            0
        }
    }

    fn col_clamp(&mut self) {
        if self.pos.col > self.size.cols - 1 {
            self.pos.col = self.size.cols - 1;
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Pos {
    pub row: u16,
    pub col: u16,
}
