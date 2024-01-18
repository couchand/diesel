use super::Bencher;
use aykroyd::{FromRow, Query, Statement};

#[cfg(feature = "postgres")]
type TestConnection = aykroyd::postgres::Client;

#[cfg(feature = "mysql")]
type TestConnection = aykroyd::mysql::Client;

#[cfg(feature = "sqlite")]
type TestConnection = aykroyd::rusqlite::Client;

#[derive(FromRow)]
#[aykroyd(by_index)]
pub struct User {
    pub id: i32,
    pub name: String,
    pub hair_color: Option<String>,
}

#[derive(Query)]
#[aykroyd(row(User), text = "SELECT id, name, hair_color FROM users")]
pub struct GetAllUsers;

#[derive(Query)]
#[aykroyd(row(UserAndPost), text = "
    SELECT user.id, user.name, user.hair_color,
        post.id, post.user_id, post.title, post.body
    FROM users AS user
    LEFT OUTER JOIN posts AS post ON post.user_id = user.id
    WHERE hair_color = $1
")]
pub struct GetUserAndPostByHairColor<'a>(&'a str);

#[derive(FromRow)]
#[aykroyd(by_index)]
pub struct UserAndPost {
    #[aykroyd(nested)]
    pub user: User,
    #[aykroyd(nested)]
    pub post: Option<Post>,
}

#[cfg(feature = "postgres")]
#[derive(Statement)]
#[aykroyd(text = "INSERT INTO users (name, hair_color) VALUES (unnest($1::text[]), unnest($2::text[]))")]
pub struct NewUsers {
    pub name: Vec<String>,
    pub hair_color: Vec<Option<String>>,
}

#[cfg(not(feature = "postgres"))]
#[derive(Statement)]
#[aykroyd(text = "INSERT INTO users (name, hair_color) VALUES ($1, $2)")]
pub struct NewUser<'a> {
    pub name: String,
    pub hair_color: Option<&'a str>,
}

#[derive(FromRow)]
#[aykroyd(by_index)]
pub struct Post {
    pub id: i32,
    pub user_id: i32,
    pub title: String,
    pub body: Option<String>,
}

#[derive(Statement)]
#[aykroyd(text = "INSERT INTO posts (user_id, title, body) VALUES (unnest($1), unnest($2), unnest($3))")]
pub struct NewPosts {
    pub user_id: Vec<i32>,
    pub title: Vec<String>,
    pub body: Vec<Option<String>>,
}

#[derive(FromRow)]
#[aykroyd(by_index)]
pub struct Comment {
    pub id: i32,
    pub post_id: i32,
    pub text: String,
}

#[derive(Statement)]
#[aykroyd(text = "INSERT INTO comments (post_id, text) VALUES (unnest($1), unnest($2))")]
pub struct NewComments(
    pub Vec<i32>,
    pub Vec<String>,
);

#[cfg(feature = "mysql")]
fn connection() -> TestConnection {
    dotenvy::dotenv().ok();
    let connection_url = dotenvy::var("MYSQL_DATABASE_URL")
        .or_else(|_| dotenvy::var("DATABASE_URL"))
        .expect("DATABASE_URL must be set in order to run tests");
    let mut conn = TestConnection::new(&connection_url).unwrap();

    #[derive(Statement)]
    #[aykroyd(text = "SET FOREIGN_KEY_CHECKS = ?")]
    struct SetForeignKeyChecks(bool);
    #[derive(Statement)]
    #[aykroyd(text = "TRUNCATE TABLE comments")]
    struct TruncateComments;
    #[derive(Statement)]
    #[aykroyd(text = "TRUNCATE TABLE posts")]
    struct TruncatePosts;
    #[derive(Statement)]
    #[aykroyd(text = "TRUNCATE TABLE users")]
    struct TruncateUsers;

    conn.execute(&SetForeignKeyChecks(false)).unwrap();
    conn.execute(&TruncateComments).unwrap();
    conn.execute(&TruncatePosts).unwrap();
    conn.execute(&TruncateUsers).unwrap();
    conn.execute(&SetForeignKeyChecks(true)).unwrap();

    conn
}

#[cfg(feature = "postgres")]
fn connection() -> TestConnection {
    dotenvy::dotenv().ok();
    let connection_url = dotenvy::var("PG_DATABASE_URL")
        .or_else(|_| dotenvy::var("DATABASE_URL"))
        .expect("DATABASE_URL must be set in order to run tests");
    let mut conn = TestConnection::connect(&connection_url, rust_postgres::NoTls).unwrap();

    #[derive(Statement)]
    #[aykroyd(text = "TRUNCATE TABLE comments CASCADE")]
    struct TruncateComments;
    #[derive(Statement)]
    #[aykroyd(text = "TRUNCATE TABLE posts CASCADE")]
    struct TruncatePosts;
    #[derive(Statement)]
    #[aykroyd(text = "TRUNCATE TABLE users CASCADE")]
    struct TruncateUsers;

    conn.execute(&TruncateComments).unwrap();
    conn.execute(&TruncatePosts).unwrap();
    conn.execute(&TruncateUsers).unwrap();

    conn
}

#[cfg(feature = "sqlite")]
fn connection() -> TestConnection {
    dotenvy::dotenv().ok();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    for migration in super::SQLITE_MIGRATION_SQL {
        conn.execute(migration, []).unwrap();
    }
    let mut conn = TestConnection::from(conn);

    #[derive(Statement)]
    #[aykroyd(text = "DELETE FROM comments")]
    struct TruncateComments;
    #[derive(Statement)]
    #[aykroyd(text = "DELETE FROM posts")]
    struct TruncatePosts;
    #[derive(Statement)]
    #[aykroyd(text = "DELETE FROM users")]
    struct TruncateUsers;

    conn.execute(&TruncateComments).unwrap();
    conn.execute(&TruncatePosts).unwrap();
    conn.execute(&TruncateUsers).unwrap();

    conn
}

#[cfg(feature = "postgres")]
fn insert_users<F: Fn(usize) -> Option<&'static str>, const N: usize>(
    conn: &mut TestConnection,
    hair_color_init: F,
) {
    let mut new_users = NewUsers {
        name: Vec::with_capacity(N),
        hair_color: Vec::with_capacity(N),
    };

    for idx in 0..N {
        new_users.name.push(format!("User {}", idx));
        new_users.hair_color.push(hair_color_init(idx).map(ToString::to_string));
    }

    conn.execute(&new_users).unwrap();
}

#[cfg(not(feature = "postgres"))]
fn insert_users<F: Fn(usize) -> Option<&'static str>, const N: usize>(
    conn: &mut TestConnection,
    hair_color_init: F,
) {
    let mut new_user = NewUser {
        name: String::new(),
        hair_color: None,
    };

    for idx in 0..N {
        new_user.name = format!("User {}", idx);
        new_user.hair_color = hair_color_init(idx);

        conn.execute(&new_user).unwrap();
    }
}

pub fn bench_trivial_query(b: &mut Bencher, size: usize) {
    let mut conn = connection();
    match size {
        1 => insert_users::<_, 1>(&mut conn, |_| None),
        10 => insert_users::<_, 10>(&mut conn, |_| None),
        100 => insert_users::<_, 100>(&mut conn, |_| None),
        1_000 => insert_users::<_, 1_000>(&mut conn, |_| None),
        10_000 => insert_users::<_, 10_000>(&mut conn, |_| None),
        _ => unimplemented!(),
    }

    b.iter(|| conn.query(&GetAllUsers).unwrap())
}

pub fn bench_medium_complex_query(b: &mut Bencher, size: usize) {
    let mut conn = connection();
    let hair_color_callback = |i| Some(if i % 2 == 0 { "black" } else { "brown" });
    match size {
        1 => insert_users::<_, 1>(&mut conn, hair_color_callback),
        10 => insert_users::<_, 10>(&mut conn, hair_color_callback),
        100 => insert_users::<_, 100>(&mut conn, hair_color_callback),
        1_000 => insert_users::<_, 1_000>(&mut conn, hair_color_callback),
        10_000 => insert_users::<_, 10_000>(&mut conn, hair_color_callback),
        _ => unimplemented!(),
    }

    b.iter(|| conn.query(&GetUserAndPostByHairColor("black")).unwrap())
}

pub fn bench_insert(b: &mut Bencher, size: usize) {
    let conn = &mut connection();

    #[inline(always)]
    fn hair_color_callback(_: usize) -> Option<&'static str> {
        Some("hair_color")
    }

    let insert: fn(&mut TestConnection) = match size {
        1 => |conn| insert_users::<_, 1>(conn, hair_color_callback),
        10 => |conn| insert_users::<_, 10>(conn, hair_color_callback),
        25 => |conn| insert_users::<_, 25>(conn, hair_color_callback),
        50 => |conn| insert_users::<_, 50>(conn, hair_color_callback),
        100 => |conn| insert_users::<_, 100>(conn, hair_color_callback),
        _ => unimplemented!(),
    };
    let insert = &insert;

    b.iter(|| {
        let insert = insert;
        insert(conn)
    })
}

/*
pub fn loading_associations_sequentially(b: &mut Bencher) {
    #[cfg(feature = "sqlite")]
    const USER_NUMBER: usize = 9;

    #[cfg(not(feature = "sqlite"))]
    const USER_NUMBER: usize = 100;

    // SETUP A TON OF DATA
    let mut conn = connection();

    insert_users::<_, USER_NUMBER>(&mut conn, |i| {
        Some(if i % 2 == 0 { "black" } else { "brown" })
    });

    let all_users = users::table.load::<User>(&mut conn).unwrap();
    let data: Vec<_> = all_users
        .iter()
        .flat_map(|user| {
            let user_id = user.id;
            (0..10).map(move |i| {
                let title = format!("Post {} by user {}", i, user_id);
                NewPost::new(user_id, &title, None)
            })
        })
        .collect();
    insert_into(posts::table)
        .values(&data)
        .execute(&mut conn)
        .unwrap();
    let all_posts = posts::table.load::<Post>(&mut conn).unwrap();
    let data: Vec<_> = all_posts
        .iter()
        .flat_map(|post| {
            let post_id = post.id;
            (0..10).map(move |i| {
                let title = format!("Comment {} on post {}", i, post_id);
                (title, post_id)
            })
        })
        .collect();
    let comment_data: Vec<_> = data
        .iter()
        .map(|&(ref title, post_id)| NewComment(post_id, &title))
        .collect();
    insert_into(comments::table)
        .values(&comment_data)
        .execute(&mut conn)
        .unwrap();

    // ACTUAL BENCHMARK
    b.iter(|| {
        let users = users::table.load::<User>(&mut conn).unwrap();
        let posts = Post::belonging_to(&users).load::<Post>(&mut conn).unwrap();
        let comments = Comment::belonging_to(&posts)
            .load::<Comment>(&mut conn)
            .unwrap()
            .grouped_by(&posts);
        let posts_and_comments = posts.into_iter().zip(comments).grouped_by(&users);
        users
            .into_iter()
            .zip(posts_and_comments)
            .collect::<Vec<(User, Vec<(Post, Vec<Comment>)>)>>()
    })
}
*/
