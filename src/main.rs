mod archive;
mod favorites;
mod rename;

use std::{collections::HashSet, fs, io::{self, stdout, Read}, path::{Path, PathBuf}, process::Command, time::Duration};
use anyhow::{Context, Result};
use crossterm::{event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::{backend::CrosstermBackend, layout::{Constraint, Direction, Layout, Rect}, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap}, Frame, Terminal};
use favorites::Favorite;
use rename::RenameRule;

#[derive(Clone)]
struct Entry { path: PathBuf, name: String, is_dir: bool, size: u64 }

struct Panel { cwd: PathBuf, items: Vec<Entry>, state: ListState, selected: HashSet<PathBuf> }
impl Panel {
    fn new(path: PathBuf) -> Self { let mut s=Self{cwd:path,items:vec![],state:ListState::default(),selected:HashSet::new()}; let _=s.refresh(); s }
    fn refresh(&mut self) -> Result<()> {
        let mut v=Vec::new();
        for e in fs::read_dir(&self.cwd).with_context(|| format!("Cannot read {}", self.cwd.display()))? {
            let e=e?; let p=e.path(); let md=e.metadata()?; v.push(Entry{ name:e.file_name().to_string_lossy().into_owned(), path:p, is_dir:md.is_dir(), size:md.len() });
        }
        v.sort_by(|a,b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        self.items=v; if self.items.is_empty(){self.state.select(None)} else { let i=self.state.selected().unwrap_or(0).min(self.items.len()-1); self.state.select(Some(i)); } Ok(())
    }
    fn current(&self)->Option<&Entry>{self.state.selected().and_then(|i|self.items.get(i))}
    fn move_sel(&mut self,d:i32){ if self.items.is_empty(){return} let cur=self.state.selected().unwrap_or(0) as i32; let n=(cur+d).clamp(0,self.items.len() as i32-1) as usize; self.state.select(Some(n)); }
    fn chosen(&self)->Vec<PathBuf>{ if self.selected.is_empty(){self.current().map(|e|vec![e.path.clone()]).unwrap_or_default()}else{self.selected.iter().cloned().collect()} }
    fn enter(&mut self)->Result<()> { if let Some(e)=self.current().cloned(){ if e.is_dir { self.cwd=e.path; self.selected.clear(); self.state.select(Some(0)); self.refresh()?; } } Ok(()) }
    fn up(&mut self)->Result<()> { if let Some(p)=self.cwd.parent(){self.cwd=p.to_path_buf();self.selected.clear();self.refresh()?;} Ok(()) }
}

#[derive(Clone,Copy,PartialEq,Eq)] enum Focus{Left,Right}
#[derive(Clone,Copy,PartialEq,Eq)] enum Mode{Normal,Favorites,Rename,Input}
#[derive(Clone,Copy)] enum PendingInput{ZipName,NewName,FavoriteName}
struct App {
    left:Panel,right:Panel,focus:Focus,mode:Mode,msg:String,clipboard:Vec<PathBuf>,cut:bool,
    favorites:Vec<Favorite>,fav_state:ListState,input:String,pending:Option<PendingInput>,
    rename_rule:RenameRule,rename_field:usize,quit:bool,
}
impl App {
    fn new()->Self{ let cwd=std::env::current_dir().unwrap_or_else(|_|PathBuf::from(".")); let mut fs=ListState::default(); fs.select(Some(0)); Self{left:Panel::new(cwd.clone()),right:Panel::new(cwd),focus:Focus::Left,mode:Mode::Normal,msg:"Ready".into(),clipboard:vec![],cut:false,favorites:favorites::load(),fav_state:fs,input:String::new(),pending:None,rename_rule:RenameRule::default(),rename_field:0,quit:false} }
    fn active(&self)->&Panel{if self.focus==Focus::Left{&self.left}else{&self.right}}
    fn active_mut(&mut self)->&mut Panel{if self.focus==Focus::Left{&mut self.left}else{&mut self.right}}
    fn other(&self)->&Panel{if self.focus==Focus::Left{&self.right}else{&self.left}}
    fn both_refresh(&mut self){let _=self.left.refresh();let _=self.right.refresh();}
}

fn main()->Result<()> {
    enable_raw_mode()?; let mut out=stdout(); execute!(out,EnterAlternateScreen)?; let backend=CrosstermBackend::new(out); let mut term=Terminal::new(backend)?;
    let result=run(&mut term);
    disable_raw_mode()?; execute!(term.backend_mut(),LeaveAlternateScreen)?; term.show_cursor()?; result
}

fn run(term:&mut Terminal<CrosstermBackend<std::io::Stdout>>)->Result<()> {
    let mut app=App::new();
    while !app.quit { term.draw(|f|draw(f,&mut app))?; if event::poll(Duration::from_millis(120))? { if let Event::Key(k)=event::read()? { if k.kind==KeyEventKind::Press { handle_key(&mut app,k)?; } } } }
    Ok(())
}

fn handle_key(app:&mut App,k:KeyEvent)->Result<()> {
    match app.mode { Mode::Normal=>normal_key(app,k), Mode::Favorites=>favorites_key(app,k), Mode::Rename=>rename_key(app,k), Mode::Input=>input_key(app,k) }
}

fn normal_key(app:&mut App,k:KeyEvent)->Result<()> {
    let ctrl=k.modifiers.contains(KeyModifiers::CONTROL) || k.modifiers.contains(KeyModifiers::SUPER);
    match (k.code,ctrl) {
        (KeyCode::Char('q'),true)=>app.quit=true,
        (KeyCode::Tab,_)=>app.focus=if app.focus==Focus::Left{Focus::Right}else{Focus::Left},
        (KeyCode::Up,_)=>app.active_mut().move_sel(-1),(KeyCode::Down,_)=>app.active_mut().move_sel(1),
        (KeyCode::PageUp,_)=>app.active_mut().move_sel(-10),(KeyCode::PageDown,_)=>app.active_mut().move_sel(10),
        (KeyCode::Enter,_)=>{app.active_mut().enter()?;},(KeyCode::Backspace,_)=>{app.active_mut().up()?;},
        (KeyCode::Char(' '),_)=>{if let Some(p)=app.active().current().map(|e|e.path.clone()){if !app.active_mut().selected.insert(p.clone()){app.active_mut().selected.remove(&p);}}},
        (KeyCode::F(4),_)=>edit_micro(app)?,(KeyCode::F(5),_)=>copy_to_other(app,false)?,(KeyCode::F(6),_)=>copy_to_other(app,true)?,
        (KeyCode::Char('c'),true)=>{app.clipboard=app.active().chosen();app.cut=false;app.msg=format!("{} item(s) copied",app.clipboard.len());},
        (KeyCode::Char('x'),true)=>{app.clipboard=app.active().chosen();app.cut=true;app.msg=format!("{} item(s) cut",app.clipboard.len());},
        (KeyCode::Char('v'),true)=>paste(app)?,
        (KeyCode::Char('r'),true)=>{app.rename_rule=RenameRule::default();app.mode=Mode::Rename;app.rename_field=0;},
        (KeyCode::Char('b'),true)=>{app.favorites=favorites::load();app.fav_state.select(if app.favorites.is_empty(){None}else{Some(0)});app.mode=Mode::Favorites;},
        (KeyCode::Char('a'),true)=>{app.pending=Some(PendingInput::ZipName);app.input="archive.zip".into();app.mode=Mode::Input;},
        (KeyCode::Char('e'),true)=>extract_current(app)?,
        (KeyCode::F(2),_)=>{if let Some(e)=app.active().current(){app.input=e.name.clone();app.pending=Some(PendingInput::NewName);app.mode=Mode::Input;}},
        (KeyCode::Char('l'),true)=>{let cwd=app.active().cwd.clone();favorites::add(&mut app.favorites,&cwd)?;app.msg="Favorite added".into();},
        _=>{}
    } Ok(())
}

fn favorites_key(app:&mut App,k:KeyEvent)->Result<()> {
    match k.code {
        KeyCode::Esc=>app.mode=Mode::Normal,
        KeyCode::Up=>move_list(&mut app.fav_state,app.favorites.len(),-1),KeyCode::Down=>move_list(&mut app.fav_state,app.favorites.len(),1),
        KeyCode::Enter=>{if let Some(i)=app.fav_state.selected(){if let Some(path)=app.favorites.get(i).map(|f| f.path.clone()){if path.is_dir(){app.active_mut().cwd=path;app.active_mut().refresh()?;app.mode=Mode::Normal;}else{app.msg="Favorite path does not exist".into();}}}},
        KeyCode::Char('a')|KeyCode::Char('A')=>{app.input=app.active().cwd.file_name().and_then(|s|s.to_str()).unwrap_or("Favorite").to_string();app.pending=Some(PendingInput::FavoriteName);app.mode=Mode::Input;},
        KeyCode::Delete=>{if let Some(i)=app.fav_state.selected(){if i<app.favorites.len(){app.favorites.remove(i);favorites::save(&app.favorites)?;app.fav_state.select(if app.favorites.is_empty(){None}else{Some(i.min(app.favorites.len()-1))});}}},
        _=>{}
    } Ok(())
}

fn rename_key(app:&mut App,k:KeyEvent)->Result<()> {
    match k.code {
        KeyCode::Esc=>app.mode=Mode::Normal,
        KeyCode::Tab=>app.rename_field=(app.rename_field+1)%7,
        KeyCode::BackTab=>app.rename_field=(app.rename_field+6)%7,
        KeyCode::Enter=>{let paths=app.active().chosen();match rename::plan(&paths,&app.rename_rule).and_then(|p|rename::apply(&p)){Ok(_)=>{app.msg=format!("Renamed {} item(s)",paths.len());app.both_refresh();app.mode=Mode::Normal},Err(e)=>app.msg=e.to_string()}},
        KeyCode::Backspace=>edit_rename_text(app,true,None),
        KeyCode::Char(c)=>edit_rename_text(app,false,Some(c)),
        _=>{}
    } Ok(())
}
fn edit_rename_text(app:&mut App,back:bool,ch:Option<char>){ let s=match app.rename_field{0=>&mut app.rename_rule.template,1=>&mut app.rename_rule.find,2=>&mut app.rename_rule.replace,3=>&mut app.rename_rule.prefix,4=>&mut app.rename_rule.suffix,5=>&mut app.rename_rule.extension,_=>return}; if back{s.pop();}else if let Some(c)=ch{s.push(c);} }

fn input_key(app:&mut App,k:KeyEvent)->Result<()> {
    match k.code {
        KeyCode::Esc=>{app.mode=Mode::Normal;app.pending=None;},KeyCode::Backspace=>{app.input.pop();},KeyCode::Char(c)=>app.input.push(c),
        KeyCode::Enter=>{let pending=app.pending.take();let input=app.input.trim().to_string();match pending{
            Some(PendingInput::ZipName)=>{let dest=app.active().cwd.join(if input.to_lowercase().ends_with(".zip"){input}else{format!("{input}.zip")});let paths=app.active().chosen();archive::compress(&paths,&dest)?;app.msg=format!("Created {}",dest.display());app.both_refresh();},
            Some(PendingInput::NewName)=>{if let Some(e)=app.active().current().cloned(){let dest=e.path.parent().unwrap_or(Path::new(".")).join(input);fs::rename(e.path,&dest)?;app.msg="Renamed".into();app.both_refresh();}},
            Some(PendingInput::FavoriteName)=>{let p=app.active().cwd.clone();app.favorites.push(Favorite{name:input,path:p});favorites::save(&app.favorites)?;app.msg="Favorite added".into();},None=>{}
        }app.mode=Mode::Normal;}
        _=>{}
    } Ok(())
}

fn move_list(s:&mut ListState,len:usize,d:i32){if len==0{return}let c=s.selected().unwrap_or(0) as i32;s.select(Some((c+d).clamp(0,len as i32-1) as usize));}

fn copy_to_other(app:&mut App,mv:bool)->Result<()> { let paths=app.active().chosen();let dest=app.other().cwd.clone();for p in &paths{copy_one(p,&dest)?;if mv{remove_path(p)?;}}app.msg=format!("{} {} item(s)",if mv{"Moved"}else{"Copied"},paths.len());app.both_refresh();Ok(()) }
fn paste(app:&mut App)->Result<()> { let dest=app.active().cwd.clone();let items=app.clipboard.clone();for p in &items{copy_one(p,&dest)?;if app.cut{remove_path(p)?;}}if app.cut{app.clipboard.clear();app.cut=false;}app.msg=format!("Pasted {} item(s)",items.len());app.both_refresh();Ok(()) }
fn copy_one(src:&Path,dest_dir:&Path)->Result<()> { let name=src.file_name().context("Invalid source")?;let dest=dest_dir.join(name);if src.is_dir(){copy_dir(src,&dest)}else{fs::copy(src,dest)?;Ok(())} }
fn copy_dir(src:&Path,dst:&Path)->Result<()> {fs::create_dir_all(dst)?;for e in fs::read_dir(src)?{let e=e?;let p=e.path();let d=dst.join(e.file_name());if p.is_dir(){copy_dir(&p,&d)?;}else{fs::copy(&p,&d)?;}}Ok(())}
fn remove_path(p:&Path)->Result<()> {if p.is_dir(){fs::remove_dir_all(p)?}else{fs::remove_file(p)?}Ok(())}
fn extract_current(app:&mut App)->Result<()> { if let Some(e)=app.active().current().cloned(){if e.path.extension().and_then(|x|x.to_str()).map(|x|x.eq_ignore_ascii_case("zip")).unwrap_or(false){let out=e.path.parent().unwrap_or(Path::new(".")).join(e.path.file_stem().unwrap_or_default());archive::extract(&e.path,&out)?;app.msg=format!("Extracted to {}",out.display());app.both_refresh();}}Ok(()) }

fn edit_micro(app:&mut App)->Result<()> {let Some(p)=app.active().current().filter(|e|!e.is_dir).map(|e|e.path.clone()) else{return Ok(())};disable_raw_mode()?;execute!(stdout(),LeaveAlternateScreen)?;let _=Command::new("micro").arg(&p).status();execute!(stdout(),EnterAlternateScreen)?;enable_raw_mode()?;app.both_refresh();Ok(())}

fn preview(path:&Path)->String { if path.is_dir(){return "Directory".into()}let mut f=match fs::File::open(path){Ok(f)=>f,Err(_)=>return "Preview unavailable".into()};let mut b=vec![0u8;32768];let n=f.read(&mut b).unwrap_or(0);b.truncate(n);if b.iter().take(4096).any(|&x|x==0){return format!("Binary file\n{} bytes",fs::metadata(path).map(|m|m.len()).unwrap_or(0))}String::from_utf8_lossy(&b).into_owned()}
fn human(n:u64)->String{if n>=1_073_741_824{format!("{:.1}G",n as f64/1_073_741_824.0)}else if n>=1_048_576{format!("{:.1}M",n as f64/1_048_576.0)}else if n>=1024{format!("{:.1}K",n as f64/1024.0)}else{format!("{n}")}}

fn draw(f:&mut Frame,app:&mut App){let area=f.area();let rows=Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(2),Constraint::Min(8),Constraint::Length(2)]).split(area);
 let title=Paragraph::new(Line::from(vec![Span::styled("DBP",Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),Span::raw(" [Dual Browser Panels]   Tab panel · F5 copy · F6 move · F4 Micro · Ctrl+R Batch Rename · Ctrl+B Favorites")])).block(Block::default().borders(Borders::BOTTOM));f.render_widget(title,rows[0]);
 let cols=Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(38),Constraint::Percentage(38),Constraint::Percentage(24)]).split(rows[1]);
 draw_panel(f,cols[0],&mut app.left,app.focus==Focus::Left);draw_panel(f,cols[1],&mut app.right,app.focus==Focus::Right);
 let prev=app.active().current().map(|e|preview(&e.path)).unwrap_or_default();f.render_widget(Paragraph::new(prev).wrap(Wrap{trim:false}).block(Block::default().title(" Preview ").borders(Borders::ALL)),cols[2]);
 f.render_widget(Paragraph::new(app.msg.clone()).block(Block::default().borders(Borders::TOP)),rows[2]);
 match app.mode{Mode::Favorites=>draw_favorites(f,area,app),Mode::Rename=>draw_rename(f,area,app),Mode::Input=>draw_input(f,area,app),_=>{}}
}
fn draw_panel(f:&mut Frame,a:Rect,p:&mut Panel,active:bool){let items:Vec<ListItem>=p.items.iter().map(|e|{let mark=if p.selected.contains(&e.path){"*"}else{" "};let icon=if e.is_dir{"D"}else{" "};ListItem::new(format!("{mark}{icon} {:<36} {:>8}",e.name,if e.is_dir{"".into()}else{human(e.size)}))}).collect();let border=if active{Style::default().fg(Color::White)}else{Style::default().fg(Color::DarkGray)};let l=List::new(items).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).block(Block::default().title(format!(" {} ",p.cwd.display())).borders(Borders::ALL).border_style(border));f.render_stateful_widget(l,a,&mut p.state);}
fn popup(area:Rect,w:u16,h:u16)->Rect{let v=Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100-h)/2),Constraint::Percentage(h),Constraint::Percentage((100-h)/2)]).split(area);Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100-w)/2),Constraint::Percentage(w),Constraint::Percentage((100-w)/2)]).split(v[1])[1]}
fn draw_favorites(f:&mut Frame,area:Rect,app:&mut App){let a=popup(area,70,65);f.render_widget(ratatui::widgets::Clear,a);let items:Vec<ListItem>=app.favorites.iter().map(|x|ListItem::new(format!("{}  —  {}",x.name,x.path.display()))).collect();let l=List::new(items).highlight_style(Style::default().add_modifier(Modifier::REVERSED)).block(Block::default().title(" Favorite Locations · Enter open · A add · Del remove · Esc close ").borders(Borders::ALL));f.render_stateful_widget(l,a,&mut app.fav_state);}
fn draw_rename(f:&mut Frame,area:Rect,app:&App){let a=popup(area,78,72);f.render_widget(ratatui::widgets::Clear,a);let paths=app.active().chosen();let plans=rename::plan(&paths,&app.rename_rule).ok();let sample=plans.as_ref().map(|v|v.iter().take(8).map(|p|format!("{}  ->  {}",p.from.file_name().unwrap_or_default().to_string_lossy(),p.to.file_name().unwrap_or_default().to_string_lossy())).collect::<Vec<_>>().join("\n")).unwrap_or_else(||"Invalid rule".into());let labels=[("Template",&app.rename_rule.template),("Find",&app.rename_rule.find),("Replace",&app.rename_rule.replace),("Prefix",&app.rename_rule.prefix),("Suffix",&app.rename_rule.suffix),("Extension",&app.rename_rule.extension)];let mut lines=Vec::new();for (i,(n,v)) in labels.iter().enumerate(){lines.push(Line::from(vec![Span::styled(format!("{n:10}: "),if i==app.rename_field{Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)}else{Style::default()}),Span::raw(v.to_string())]));}lines.push(Line::raw(""));lines.push(Line::styled("Tokens: [N] processed · [O] original stem · [F] full · [E] ext · [C]/[C3] counter",Style::default().fg(Color::DarkGray)));lines.push(Line::raw(""));for s in sample.lines(){lines.push(Line::raw(s.to_string()));}lines.push(Line::raw(""));lines.push(Line::raw("Tab next field · Enter APPLY · Esc cancel"));f.render_widget(Paragraph::new(lines).wrap(Wrap{trim:false}).block(Block::default().title(" Batch Rename ").borders(Borders::ALL)),a);}
fn draw_input(f:&mut Frame,area:Rect,app:&App){let a=popup(area,60,24);f.render_widget(ratatui::widgets::Clear,a);let title=match app.pending{Some(PendingInput::ZipName)=>" Compress to ZIP ",Some(PendingInput::NewName)=>" Rename ",Some(PendingInput::FavoriteName)=>" Favorite name ",None=>" Input "};f.render_widget(Paragraph::new(app.input.clone()).block(Block::default().title(title).borders(Borders::ALL)),a);}
