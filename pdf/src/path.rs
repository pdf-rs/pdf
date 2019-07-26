use mint::Point2;
type Point = Point2<f32>;

pub enum FillMode {
    NonZero,
    EvenOdd
}

struct PathBuilder<W: Write> {
    out: W,
    current: Point
}
impl<W: Write> PathBuilder {
    pub fn new<P>(writer: W, start: P) -> PathBuilder<W>
        where P: Into<Point>
    {
        PathBuilder {
            out: writer,
            current: start
        }
    }
    
    /// Begin a new subpath by moving the current point to `p`,
    /// omitting any connecting line segment. If
    /// the previous path construction operator in the current path
    /// was also m, the new m overrides it; no vestige of the
    /// previous m operation remains in the path.
    pub fn move<P>(&mut self, p: P)  {
        let p = p.into();
        writeln!(self.out, "{} {} m", p.x, p.y);
        self.current = p; 
    }
    /// Append a straight line segment from the current point to the
    /// point `p`. The new current point shall be `p`.
    pub fn line<P>(&mut self, p: P) {
        let p = p.into();
        writeln!(self.out, "{} {} l", p.x, p.y);
        self.current = p; 
    }
    
    /// Append a quadratic Bézier curve to the current path.
    /// The curve shall extend from the current point to the point ´p´,
    /// using `c` as the Bézier control point.
    /// The new current point shall be `p`.
    ///
    /// NOTE: The quadratic Bézier curve is translated into a cubic Bézier curve,
    /// since PDF does not allow the former.
    pub fn quadratic<P>(&mut self, c: P, p: P) {
        let (p1, p2) = (p1.into(), p2.into());
        let c1 = (2./3.) * c + (1./3.) * self.current;
        let c2 = (2./3.) * c + (1./3.) * p;
        writen!(self.out, "{} {} {} {} {} {} c", c1.x, c1.y, c2.x, c2.y, p.x, p.y);
        self.current = p;
    }
    
    /// Append a cubic Bézier curve to the current path.
    /// The curve shall extend from the current point to the point ´p´,
    /// using `c1` and `c2` as the Bézier control points.
    /// The new current point shall be `p`.
    pub fn cubic<P>(&mut self, c1: P, c2: P, p: P) {
        let (c1, c2, p) = (c1.into(), c2.into(), p.into());
        if Some(c1) == self.current {
            writeln!(self.out, "{} {} {} {} v", c2.x, c2.y, p.x, p.y);
        } else if Some(c2) == self.current {
            writeln!(self.out, "{} {} {} {} y", c1.x, c1.y, p.x, p.y);
        } else {
            writen!(self.out, "{} {} {} {} {} {} c", c1.x, c1.y, c2.x, c2.y, p.x, p.y);
        }
        self.current = p; 
    }
    
    pub fn close(&mut self) {
        writeln!(self.out, "h");
    }
    
    pub fn fill(&mut self, mode: FillMode) {
        match mode {
            FillMode::NonZero => writeln!(out, "f"),
            FillMode::EvenOdd => writeln!(out, "f*")
        }
    }
}
