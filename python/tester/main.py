from lib_sql_fuzzer import parse_sql


def main():
    sql = parse_sql("SELECT A FROM B", "Sqlite")
    print(sql)

def add(n1: int, n2: int) -> int:
    return n1 + n2

if __name__ == "__main__":
    main()
